use crate::events::{AgentEvent, EventLog};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub async fn run_sentinel(
    event_log: Arc<EventLog>,
    cdn_cache: Option<Arc<crate::cdn::CacheManager>>,
) {
    let pid = std::process::id().to_string();

    event_log.publish(AgentEvent {
        agent_id: "sentinel".into(),
        event_type: "started".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: FxHashMap::from_iter([("pid".into(), pid)]),
    });

    let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
    tick.tick().await;

    loop {
        tick.tick().await;

        if let Some(ref cache) = cdn_cache {
            let stats = cache.stats();
            if let Some(evictions) = stats.get("evictions") {
                if *evictions > 0 {
                    event_log.publish(AgentEvent {
                        agent_id: "sentinel".into(),
                        event_type: "cdn_scrub".into(),
                        severity: if *evictions > 1000 { "warn" } else { "info" }.into(),
                        timestamp: 0,
                        metadata: FxHashMap::from_iter([
                            ("evictions".into(), evictions.to_string()),
                            (
                                "entries".into(),
                                stats.get("entries").map(|v| v.to_string()).unwrap_or_default(),
                            ),
                        ]),
                    });
                }
            }
        }

        event_log.publish(AgentEvent {
            agent_id: "sentinel".into(),
            event_type: "health_check".into(),
            severity: "info".into(),
            timestamp: 0,
            metadata: FxHashMap::from_iter([("cdn".into(), cdn_cache.is_some().to_string())]),
        });
    }
}
