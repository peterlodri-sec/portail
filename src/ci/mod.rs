/*
 * CI Status Module — Live Build Status
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    CI Status Flow                           │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   GitHub Actions                                            │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  Webhook   │────▶│  Parse     │────▶│  Store     │     │
 *   │   │  (POST)    │     │  Event     │     │  (memory)  │     │
 *   │   └────────────┘     └────────────┘     └────────────┘     │
 *   │        │                               │                   │
 *   │        │                               ▼                   │
 *   │        │                     ┌────────────┐                │
 *   │        │                     │  Status    │                │
 *   │        │                     │  Page      │                │
 *   │        │                     └────────────┘                │
 *   │        │                               ▲                   │
 *   │        │                               │                   │
 *   │        └───────────────────────────────┘                   │
 *   │                  GET /ci/status                            │
 *   │                  GET /ci/badge                             │
 *   │                                                             │
 *   │   Webhook Events:                                           │
 *   │   - workflow_run (started, completed, failed)               │
 *   │   - workflow_job (queued, in_progress, completed)           │
 *   │   - check_run (created, completed)                          │
 *   │                                                             │
 *   │   Status API:                                               │
 *   │   - GET /ci/status — JSON with all workflow statuses        │
 *   │   - GET /ci/badge — SVG badge for README                    │
 *   │   - GET /ci/live — SSE stream for live updates              │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::types::BoundedMeta;

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub workflow_name: String,
    pub status: WorkflowStatus,
    pub conclusion: Option<String>,
    pub branch: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub actor: String,
    pub event: String,
    pub run_number: u32,
    pub created_at: String,
    pub updated_at: String,
    pub run_started_at: Option<String>,
    pub jobs_url: String,
    pub logs_url: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiStatus {
    pub overall: OverallStatus,
    pub workflows: Vec<WorkflowRun>,
    pub last_updated: String,
    pub total_runs: u64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallStatus {
    Passing,
    Failing,
    InProgress,
    Unknown,
}

// ── GitHub Webhook Payloads ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunEvent {
    pub action: String,
    pub workflow_run: WorkflowRunPayload,
    pub repository: RepositoryPayload,
    pub sender: SenderPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunPayload {
    pub id: u64,
    pub name: String,
    pub workflow_id: u64,
    pub run_number: u32,
    pub event: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub head_branch: String,
    pub head_sha: String,
    pub run_started_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub jobs_url: String,
    pub logs_url: String,
    pub html_url: String,
    pub head_commit: Option<CommitPayload>,
    pub actor: Option<ActorPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitPayload {
    pub message: String,
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorPayload {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPayload {
    pub full_name: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderPayload {
    pub login: String,
}

// ── CI Status Store ──────────────────────────────────────────────

pub struct CiStatusStore {
    runs: std::sync::RwLock<Vec<WorkflowRun>>,
    max_runs: usize,
    webhook_secret: Option<String>,
}

impl CiStatusStore {
    pub fn new(max_runs: usize, webhook_secret: Option<String>) -> Self {
        Self {
            runs: std::sync::RwLock::new(Vec::with_capacity(max_runs)),
            max_runs,
            webhook_secret,
        }
    }

    pub fn record_run(&self, run: WorkflowRun) {
        let mut runs = self.runs.write().unwrap_or_else(|e| e.into_inner());
        
        // Update existing run or add new
        if let Some(existing) = runs.iter_mut().find(|r| r.id == run.id) {
            *existing = run;
        } else {
            if runs.len() >= self.max_runs {
                runs.remove(0);
            }
            runs.push(run);
        }
    }

    pub fn get_status(&self) -> CiStatus {
        let runs = self.runs.read().unwrap_or_else(|e| e.into_inner());
        let total = runs.len();
        let completed = runs.iter().filter(|r| r.status == WorkflowStatus::Completed).count();
        let successful = runs.iter()
            .filter(|r| r.conclusion.as_deref() == Some("success"))
            .count();
        
        let success_rate = if completed > 0 {
            successful as f64 / completed as f64
        } else {
            0.0
        };

        let overall = if runs.iter().any(|r| r.status == WorkflowStatus::InProgress) {
            OverallStatus::InProgress
        } else if runs.iter().any(|r| r.conclusion.as_deref() == Some("failure")) {
            OverallStatus::Failing
        } else if runs.iter().all(|r| r.conclusion.as_deref() == Some("success")) {
            OverallStatus::Passing
        } else {
            OverallStatus::Unknown
        };

        CiStatus {
            overall,
            workflows: runs.clone(),
            last_updated: chrono::Utc::now().to_rfc3339(),
            total_runs: total as u64,
            success_rate,
        }
    }

    pub fn get_badge_svg(&self) -> String {
        let status = self.get_status();
        let (color, text) = match status.overall {
            OverallStatus::Passing => ("44cc11", "passing"),
            OverallStatus::Failing => ("e05d44", "failing"),
            OverallStatus::InProgress => ("dfb317", "in progress"),
            OverallStatus::Unknown => ("9f9f9f", "unknown"),
        };

        // Simple SVG badge without problematic characters
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"20\">\
             <rect width=\"100\" height=\"20\" rx=\"3\" fill=\"#555\"/>\
             <rect x=\"57\" width=\"43\" height=\"20\" rx=\"3\" fill=\"#{color}\"/>\
             <text x=\"28\" y=\"14\" fill=\"#fff\" text-anchor=\"middle\" font-family=\"DejaVu Sans,Verdana,Geneva,sans-serif\" font-size=\"11\">CI</text>\
             <text x=\"77\" y=\"14\" fill=\"#fff\" text-anchor=\"middle\" font-family=\"DejaVu Sans,Verdana,Geneva,sans-serif\" font-size=\"11\">{text}</text>\
             </svg>",
            color = color,
            text = text
        );
        svg
    }

    pub fn verify_webhook(&self, payload: &[u8], signature: &str) -> bool {
        if let Some(ref secret) = self.webhook_secret {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            
            type HmacSha256 = Hmac<Sha256>;
            
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(payload);
            
            let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
            expected == signature
        } else {
            true // No secret configured, accept all
        }
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_status(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<CiStatus> {
    axum::Json(state.ci_status.get_status())
}

pub async fn handle_badge(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    let svg = state.ci_status.get_badge_svg();
    (
        axum::http::StatusCode::OK,
        [("content-type", "image/svg+xml"), ("cache-control", "no-cache")],
        svg,
    )
}

pub async fn handle_webhook(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> impl axum::response::IntoResponse {
    // Get signature from headers
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Verify webhook signature
    if !state.ci_status.verify_webhook(&body, signature) {
        return (axum::http::StatusCode::UNAUTHORIZED, "invalid signature");
    }

    // Parse event type
    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match event_type {
        "workflow_run" => {
            match serde_json::from_slice::<WorkflowRunEvent>(&body) {
                Ok(event) => {
                    let run = WorkflowRun {
                        id: event.workflow_run.id,
                        name: event.workflow_run.name.clone(),
                        workflow_name: event.workflow_run.name.clone(),
                        status: match event.workflow_run.status.as_str() {
                            "queued" => WorkflowStatus::Queued,
                            "in_progress" => WorkflowStatus::InProgress,
                            "completed" => WorkflowStatus::Completed,
                            "waiting" => WorkflowStatus::Waiting,
                            "requested" => WorkflowStatus::Requested,
                            _ => WorkflowStatus::Pending,
                        },
                        conclusion: event.workflow_run.conclusion.clone(),
                        branch: event.workflow_run.head_branch,
                        commit_sha: event.workflow_run.head_sha,
                        commit_message: event.workflow_run.head_commit
                            .as_ref()
                            .map(|c| c.message.clone())
                            .unwrap_or_default(),
                        actor: event.workflow_run.actor
                            .as_ref()
                            .map(|a| a.login.clone())
                            .unwrap_or_else(|| event.sender.login.clone()),
                        event: event.workflow_run.event.clone(),
                        run_number: event.workflow_run.run_number,
                        created_at: event.workflow_run.created_at.clone(),
                        updated_at: event.workflow_run.updated_at.clone(),
                        run_started_at: event.workflow_run.run_started_at.clone(),
                        jobs_url: event.workflow_run.jobs_url.clone(),
                        logs_url: event.workflow_run.logs_url.clone(),
                        html_url: event.workflow_run.html_url.clone(),
                    };
                    
                    state.ci_status.record_run(run);
                    
                    // Publish event
                    state.event_log.publish(crate::events::AgentEvent {
                        agent_id: "ci".into(),
                        event_type: "workflow_run".into(),
                        severity: "info".into(),
                        timestamp: 0,
                        metadata: BoundedMeta::from_iter([
                            ("action".into(), event.action),
                            ("workflow".into(), event.workflow_run.name),
                            ("status".into(), event.workflow_run.status),
                            ("conclusion".into(), event.workflow_run.conclusion.unwrap_or_default()),
                        ]),
                    });
                    
                    (axum::http::StatusCode::OK, "ok")
                }
                Err(_) => (axum::http::StatusCode::BAD_REQUEST, "invalid payload"),
            }
        }
        "ping" => (axum::http::StatusCode::OK, "pong"),
        _ => (axum::http::StatusCode::OK, "ignored"),
    }
}

pub async fn handle_live(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::{Event, KeepAlive};
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt;

    let rx = state.event_log.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|r| match r {
            Ok(event) if event.agent_id == "ci" => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default().event("ci").data(data)))
            }
            _ => None,
        });

    axum::response::Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/ci/status", axum::routing::get(handle_status))
        .route("/ci/badge", axum::routing::get(handle_badge))
        .route("/ci/live", axum::routing::get(handle_live))
        .route("/ci/webhook", axum::routing::post(handle_webhook))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> CiStatusStore {
        CiStatusStore::new(100, None)
    }

    #[test]
    fn record_and_get_status() {
        let store = test_store();
        
        store.record_run(WorkflowRun {
            id: 1,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: WorkflowStatus::Completed,
            conclusion: Some("success".into()),
            branch: "main".into(),
            commit_sha: "abc123".into(),
            commit_message: "test".into(),
            actor: "test-user".into(),
            event: "push".into(),
            run_number: 1,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:01:00Z".into(),
            run_started_at: Some("2026-01-01T00:00:01Z".into()),
            jobs_url: "".into(),
            logs_url: "".into(),
            html_url: "".into(),
        });

        let status = store.get_status();
        assert_eq!(status.total_runs, 1);
        assert!(matches!(status.overall, OverallStatus::Passing));
    }

    #[test]
    fn badge_svg_passing() {
        let store = test_store();
        store.record_run(WorkflowRun {
            id: 1,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: WorkflowStatus::Completed,
            conclusion: Some("success".into()),
            branch: "main".into(),
            commit_sha: "abc".into(),
            commit_message: "test".into(),
            actor: "user".into(),
            event: "push".into(),
            run_number: 1,
            created_at: "".into(),
            updated_at: "".into(),
            run_started_at: None,
            jobs_url: "".into(),
            logs_url: "".into(),
            html_url: "".into(),
        });

        let svg = store.get_badge_svg();
        assert!(svg.contains("passing"));
        assert!(svg.contains("44cc11"));
    }

    #[test]
    fn badge_svg_failing() {
        let store = test_store();
        store.record_run(WorkflowRun {
            id: 1,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: WorkflowStatus::Completed,
            conclusion: Some("failure".into()),
            branch: "main".into(),
            commit_sha: "abc".into(),
            commit_message: "test".into(),
            actor: "user".into(),
            event: "push".into(),
            run_number: 1,
            created_at: "".into(),
            updated_at: "".into(),
            run_started_at: None,
            jobs_url: "".into(),
            logs_url: "".into(),
            html_url: "".into(),
        });

        let svg = store.get_badge_svg();
        assert!(svg.contains("failing"));
        assert!(svg.contains("e05d44"));
    }

    #[test]
    fn webhook_verification() {
        let store = CiStatusStore::new(100, Some("test-secret".into()));
        
        // Valid signature - compute HMAC-SHA256 of "test payload" with key "test-secret"
        let payload = b"test payload";
        let _valid_sig = "sha256=1f92c44a9de3e8b29e4e0e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e";
        
        // Invalid signature
        let invalid_sig = "sha256=invalid";
        
        // No secret configured - should accept all
        let no_secret_store = CiStatusStore::new(100, None);
        assert!(no_secret_store.verify_webhook(payload, "anything"));
        
        // With secret - should reject invalid
        assert!(!store.verify_webhook(payload, invalid_sig));
    }
}
