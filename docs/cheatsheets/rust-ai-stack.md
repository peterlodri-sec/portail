# Rust AI Stack — Native Production Tools (2026)

## 1. Tensor & Compute Backends

| Crate | Best For | Hardware |
|-------|----------|----------|
| `candle` (HF) | Lightweight inference, PyTorch-like API | CPU SIMD, CUDA, Metal, WASM |
| `burn` | Compile-time optimized, cross-device | WGPU, CUDA, LibTorch |
| `dfdx` | Compile-time shape verification | CPU (type-checked graphs) |

## 2. Inference Engines

| Crate | Best For | Format Support |
|-------|----------|---------------|
| `mistral.rs` | Local GGUF/Safetensors serving | GGUF, GGML, Safetensors |
| `llama-core` | llama.cpp wrapper, sandboxed | GGUF |
| `wasmedge-llama` | Edge/WASM inference | GGUF |

## 3. Orchestration

| Crate | Best For | Pattern |
|-------|----------|---------|
| `rig` | LangChain-like, type-safe | Vector stores, RAG, tool calling |
| `autoagents` | Multi-agent swarms | Actor model (ractor) |
| `openfang` | Production agent OS | WASM sandboxes, durable loops |

## Architecture

```
[ Axum / Tower HTTP ]
         │
[ Rig Orchestration ]
    ┌────┴────┐
    │         │
[Remote API] [Local mistral.rs]
(OpenAI/etc) (candle backend)
```
