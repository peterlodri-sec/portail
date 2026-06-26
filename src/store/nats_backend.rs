//! NATS-replicated backend — multi-node eventual consistency via sqlx.
//!
//! Writes to local sqlx pool AND publishes to NATS. Other nodes consume
//! and write to their local sqlx pool. Free, open source.

use futures::StreamExt;

use super::StoreBackend;
use super::StoreConfig;
use super::StoredEvent;
use super::sqlx_backend::SqlxBackend;

pub struct NatsReplicatedBackend {
    local: SqlxBackend,
    nc: async_nats::Client,
}

impl NatsReplicatedBackend {
    pub async fn open(config: &StoreConfig) -> Result<Self, String> {
        let local = SqlxBackend::open(config).await?;
        let nats_url = std::env::var("PORTAIL_NATS_URL")
            .unwrap_or_else(|_| "nats://localhost:4222".into());
        let nc = async_nats::connect(&nats_url).await.map_err(|e| e.to_string())?;

        let pool = local.pool.clone();
        let sub_nc = nc.clone();
        tokio::spawn(async move {
            let mut sub = sub_nc.subscribe("portail.store.events".to_string()).await
                .expect("NATS subscribe for store replication");
            while let Some(msg) = sub.next().await {
                if let Ok(event) = serde_json::from_slice::<StoredEvent>(&msg.payload) {
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO events (agent_id, event_type, severity, timestamp, metadata)
                         VALUES (?1, ?2, ?3, ?4, ?5)"
                    )
                    .bind(&event.agent_id)
                    .bind(&event.event_type)
                    .bind(&event.severity)
                    .bind(event.timestamp)
                    .bind(&event.metadata_json)
                    .execute(&pool)
                    .await;
                }
            }
        });

        Ok(Self { local, nc })
    }

    fn publish_to_nats(&self, event: &StoredEvent) {
        let nc = self.nc.clone();
        let payload = serde_json::to_vec(event).unwrap_or_default();
        tokio::spawn(async move {
            let _ = nc.publish("portail.store.events".to_string(), payload.into()).await;
        });
    }
}

impl StoreBackend for NatsReplicatedBackend {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        let id = self.local.insert(event)?;
        self.publish_to_nats(event);
        Ok(id)
    }
    fn query(&self, a: Option<&str>, e: Option<&str>, s: Option<i64>, l: Option<usize>)
        -> Result<Vec<StoredEvent>, String> { self.local.query(a, e, s, l) }
    fn count(&self) -> Result<i64, String> { self.local.count() }
    fn purge_expired(&self, d: u32) -> Result<usize, String> { self.local.purge_expired(d) }
    fn export_json(&self, s: Option<i64>) -> Result<String, String> { self.local.export_json(s) }
}
