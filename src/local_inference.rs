//! Local Inference — mistral.rs + candle backend
//!
//! Serves local LLM models via OpenAI-compatible API.
//! Requests routed here by gateway when model matches local config.
//!
//! Architecture:
//! ```text
//! [target:local] ──► mistral.rs engine ──► candle (Metal/CUDA/CPU)
//!                       │
//!                       ▼
//!              /v1/chat/completions (OpenAI API)
//! ```

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ── Config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalInferenceConfig {
    /// Enable local inference
    #[serde(default)]
    pub enabled: bool,

    /// Model path (GGUF or Safetensors)
    pub model_path: Option<String>,

    /// Model ID to expose via API (e.g. "local-mistral-7b")
    #[serde(default = "default_model_id")]
    pub model_id: String,

    /// Listen address for inference server
    #[serde(default = "default_listen")]
    pub listen: String,

    /// Max tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Context length
    #[serde(default = "default_context_length")]
    pub context_length: usize,

    /// GPU layers (0 = CPU only, -1 = all layers on GPU)
    #[serde(default = "default_gpu_layers")]
    pub gpu_layers: i32,
}

fn default_model_id() -> String {
    "local-model".into()
}
fn default_listen() -> String {
    "127.0.0.1:8788".into()
}
fn default_max_tokens() -> usize {
    2048
}
fn default_temperature() -> f32 {
    0.7
}
fn default_context_length() -> usize {
    4096
}
fn default_gpu_layers() -> i32 {
    -1
}

impl Default for LocalInferenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: None,
            model_id: default_model_id(),
            listen: default_listen(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            context_length: default_context_length(),
            gpu_layers: default_gpu_layers(),
        }
    }
}

// ── OpenAI-compatible types ───────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

// ── Engine state ──────────────────────────────────────────────────

pub struct InferenceEngine {
    config: LocalInferenceConfig,
    /// Placeholder — real impl uses mistralrs::MistralRs
    /// When feature "local-inference" is enabled, this holds the loaded model
    model_loaded: RwLock<bool>,
}

impl InferenceEngine {
    pub fn new(config: LocalInferenceConfig) -> Self {
        Self {
            config,
            model_loaded: RwLock::new(false),
        }
    }

    /// Load model from config path
    pub async fn load_model(&self) -> anyhow::Result<()> {
        let path = self
            .config
            .model_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no model_path configured"))?;

        info!(model_path = %path, "loading local model");

        // TODO: Real mistral.rs integration
        // ```rust
        // use mistralrs::{MistralRsBuilder, NormalRequest, VisionNormalRequest, ModelCategory};
        // let pipeline = MistralRsBuilder::new(
        //     NormalLoaderType::Llama,
        //     NormalLoaderMetadata {
        //         model_path: path.into(),
        //         tokenizer_json: None,
        //         device: Device::new_cuda(0).unwrap_or(Device::Cpu),
        //     },
        //     DeviceMapMetadata::from_num_device(1),
        //     None,  // isq
        //     None,  // pa
        // ).build()?;
        // ```

        let mut loaded = self.model_loaded.write().await;
        *loaded = true;
        info!("model loaded (stub — mistral.rs integration pending)");
        Ok(())
    }

    /// Run inference on a chat completion request
    pub async fn complete(
        &self,
        req: ChatCompletionRequest,
    ) -> anyhow::Result<ChatCompletionResponse> {
        let loaded = *self.model_loaded.read().await;
        if !loaded {
            anyhow::bail!("model not loaded");
        }

        let max_tokens = req.max_tokens.unwrap_or(self.config.max_tokens);
        let temperature = req.temperature.unwrap_or(self.config.temperature);

        // Build prompt from messages
        let prompt = req
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // TODO: Real inference via mistral.rs
        // ```rust
        // let request = NormalRequest {
        //     messages: vec![Message::new(...)],
        //     sampling_params: SamplingParams::new(temperature, max_tokens, ...),
        //     ..
        // };
        // let response = pipeline.sender().send(request).await??;
        // ```

        // Stub response
        let response_text = format!(
            "[local inference stub] model={} temp={} tokens={} prompt_len={}",
            self.config.model_id,
            temperature,
            max_tokens,
            prompt.len()
        );

        Ok(ChatCompletionResponse {
            id: format!("chatcmpl-local-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".into(),
            created: chrono::Utc::now().timestamp(),
            model: self.config.model_id.clone(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".into(),
                    content: response_text,
                },
                finish_reason: "stop".into(),
            }],
            usage: Usage {
                prompt_tokens: prompt.split_whitespace().count(),
                completion_tokens: max_tokens,
                total_tokens: prompt.split_whitespace().count() + max_tokens,
            },
        })
    }

    /// Check if model is loaded and ready
    pub async fn is_ready(&self) -> bool {
        *self.model_loaded.read().await
    }

    /// Get model info
    pub fn model_info(&self) -> serde_json::Value {
        serde_json::json!({
            "model_id": self.config.model_id,
            "model_path": self.config.model_path,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
            "context_length": self.config.context_length,
            "gpu_layers": self.config.gpu_layers,
        })
    }
}

// ── HTTP handlers ────────────────────────────────────────────────

/// POST /v1/chat/completions — OpenAI-compatible endpoint
pub async fn handle_chat_completions(
    State(state): State<Arc<crate::AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    let engine = match &state.inference_engine {
        Some(e) => e,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {"message": "local inference not enabled", "type": "service_unavailable"}
                })),
            ).into_response();
        }
    };

    match engine.complete(req).await {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(e) => {
            warn!(error = %e, "local inference failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {
                        "message": e.to_string(),
                        "type": "inference_error",
                    }
                })),
            )
                .into_response()
        }
    }
}

/// GET /v1/models — list local models
pub async fn handle_list_models(State(state): State<Arc<crate::AppState>>) -> Response {
    let engine = match &state.inference_engine {
        Some(e) => e,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "local inference not enabled"})),
            )
                .into_response();
        }
    };

    let info = engine.model_info();
    let ready = engine.is_ready().await;

    let resp = serde_json::json!({
        "object": "list",
        "data": [{
            "id": info["model_id"],
            "object": "model",
            "created": 0,
            "owned_by": "portail-local",
            "ready": ready,
        }]
    });

    (StatusCode::OK, Json(resp)).into_response()
}

/// GET /v1/health — local inference health check
pub async fn handle_health(State(state): State<Arc<crate::AppState>>) -> Response {
    let ready = match &state.inference_engine {
        Some(e) => e.is_ready().await,
        None => false,
    };
    let status = if ready { "ok" } else { "model_not_loaded" };
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(serde_json::json!({"status": status}))).into_response()
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = LocalInferenceConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.model_id, "local-model");
        assert_eq!(cfg.max_tokens, 2048);
        assert_eq!(cfg.temperature, 0.7);
        assert_eq!(cfg.gpu_layers, -1);
    }

    #[tokio::test]
    async fn engine_not_loaded_returns_error() {
        let engine = InferenceEngine::new(LocalInferenceConfig::default());
        let req = ChatCompletionRequest {
            model: "local-model".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            max_tokens: None,
            temperature: None,
            stream: None,
        };
        assert!(engine.complete(req).await.is_err());
    }

    #[tokio::test]
    async fn engine_model_info() {
        let cfg = LocalInferenceConfig {
            model_path: Some("/models/test.gguf".into()),
            model_id: "test-model".into(),
            ..Default::default()
        };
        let engine = InferenceEngine::new(cfg);
        let info = engine.model_info();
        assert_eq!(info["model_id"], "test-model");
        assert_eq!(info["model_path"], "/models/test.gguf");
    }

    #[test]
    fn chat_request_deserialize() {
        let json = r#"{
            "model": "local-model",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 100,
            "temperature": 0.5
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "local-model");
        assert_eq!(req.max_tokens, Some(100));
        assert_eq!(req.temperature, Some(0.5));
    }

    #[test]
    fn chat_response_serialize() {
        let resp = ChatCompletionResponse {
            id: "test".into(),
            object: "chat.completion".into(),
            created: 0,
            model: "m".into(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".into(),
                    content: "hi".into(),
                },
                finish_reason: "stop".into(),
            }],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("chat.completion"));
        assert!(json.contains("stop"));
    }
}
