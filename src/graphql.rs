//! GraphQL API — events, hooks, tasks with subscriptions.
//! v0.6 — async-graphql 7 with raw axum handler (no async-graphql-axum).

use async_graphql::{Context, Object, Schema, SimpleObject, Subscription};
use std::sync::Arc;

#[derive(SimpleObject, Clone)]
struct GqlEvent {
    agent_id: String,
    event_type: String,
    severity: String,
    timestamp: u64,
    metadata_json: String,
}

#[derive(SimpleObject, Clone)]
struct GqlHook {
    id: String,
    match_agent: Option<String>,
    match_path: Option<String>,
    match_event_type: Option<String>,
    enabled: bool,
}

#[derive(SimpleObject, Clone)]
struct GqlTask {
    id: String,
    status: String,
    message_count: usize,
    artifact_count: usize,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn events(&self, ctx: &Context<'_>, agent_id: Option<String>, event_type: Option<String>, #[graphql(default = 20)] limit: usize) -> Vec<GqlEvent> {
        let state = ctx.data_unchecked::<Arc<crate::AppState>>();
        state.event_log.recent(limit.min(200)).into_iter()
            .filter(|e| agent_id.as_ref().map_or(true, |a| &e.agent_id == a) && event_type.as_ref().map_or(true, |t| &e.event_type == t))
            .map(|e| GqlEvent { agent_id: e.agent_id, event_type: e.event_type, severity: e.severity, timestamp: e.timestamp, metadata_json: serde_json::to_string(&e.metadata).unwrap_or_default() })
            .collect()
    }

    async fn hooks(&self, ctx: &Context<'_>) -> Vec<GqlHook> {
        let state = ctx.data_unchecked::<Arc<crate::AppState>>();
        state.hooks.list().into_iter().map(|h| GqlHook { id: h.id, match_agent: h.match_agent, match_path: h.match_path, match_event_type: h.match_event_type, enabled: h.enabled }).collect()
    }

    async fn tasks(&self, ctx: &Context<'_>) -> Vec<GqlTask> {
        let state = ctx.data_unchecked::<Arc<crate::AppState>>();
        state.a2a_tasks.get_all().into_iter().map(|t| GqlTask { id: t.id, status: format!("{:?}", t.status), message_count: t.messages.len(), artifact_count: t.artifacts.len() }).collect()
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn publish_event(&self, ctx: &Context<'_>, agent_id: String, event_type: String, severity: Option<String>, metadata_json: Option<String>) -> async_graphql::Result<bool> {
        let state = ctx.data_unchecked::<Arc<crate::AppState>>();
        let meta: crate::types::BoundedMeta = metadata_json.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        state.event_log.publish(crate::events::AgentEvent { agent_id, event_type, severity: severity.unwrap_or_else(|| "info".into()), timestamp: 0, metadata: meta });
        Ok(true)
    }
}

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    async fn live_events<'ctx>(&self, ctx: &Context<'ctx>) -> async_graphql::Result<impl futures::Stream<Item = GqlEvent> + 'ctx> {
        let state = ctx.data_unchecked::<Arc<crate::AppState>>();
        let mut rx = state.event_log.subscribe();
        Ok(async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(event) => yield GqlEvent { agent_id: event.agent_id, event_type: event.event_type, severity: event.severity, timestamp: event.timestamp, metadata_json: serde_json::to_string(&event.metadata).unwrap_or_default() },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => { tracing::warn!(skipped = n, "graphql lagged"); continue; }
                    Err(_) => break,
                }
            }
        })
    }
}

pub type PortailSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

fn build_schema() -> PortailSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot).finish()
}

// ─── raw axum handler ─────────────────────────────────────────────

pub async fn graphql_handler(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let schema = build_schema();
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let variables = body.get("variables").cloned().unwrap_or(serde_json::Value::Null);

    let request = async_graphql::Request::new(query)
        .variables(async_graphql::Variables::from_json(variables));

    let mut data = async_graphql::Data::default();
    data.insert(state);
    let response = schema.execute(request.data(data)).await;

    let json = serde_json::to_value(&response).unwrap_or(serde_json::json!({"errors": [{"message": "serialization failed"}]}));
    axum::Json(json)
}

pub async fn graphql_playground() -> axum::response::Html<String> {
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"),
    ))
}

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/graphql", axum::routing::post(graphql_handler))
        .route("/graphql", axum::routing::get(graphql_playground))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_builds() {
        let schema = build_schema();
        assert!(schema.sdl().contains("QueryRoot"));
    }

    #[test]
    fn schema_has_events() {
        let schema = build_schema();
        let sdl = schema.sdl();
        assert!(sdl.contains("events"));
        assert!(sdl.contains("hooks"));
        assert!(sdl.contains("tasks"));
    }
}
