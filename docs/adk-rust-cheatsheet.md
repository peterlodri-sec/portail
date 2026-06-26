# ADK-Rust Cheatsheet — zavora-ai/adk-rust v1.0.0

**Crate:** `adk-rust` — meta-crate with feature flags.
**Repo:** https://github.com/zavora-ai/adk-rust
**Docs:** https://docs.rs/adk-rust/latest/adk_rust/

---

## Feature Flags

| Feature | Pulls in | Use case |
|---------|----------|----------|
| (default) | agents, models, tools, sessions, runner | Standard LLM agent |
| `full` | +graph, realtime, browser, eval, rag | All stable crates |
| `labs` | +code, sandbox, audio | Experimental |
| `minimal` | agents, gemini, runner | Fastest build |
| Custom | Pick any: `agents`, `gemini`, `tools`, `sessions`, `openai`, `anthropic` | Precise deps |

**Cargo.toml:**
```toml
adk-rust = "1.0"
# or fine-grained:
adk-core = "1.0"
adk-agent = "1.0"
adk-model = "1.0"
adk-tool = "1.0"
adk-runner = "1.0"
adk-session = "1.0"
adk-graph = "1.0"
```

---

## Imports

```rust
// Everything via meta-crate prelude:
use adk_rust::prelude::*;

// Individual crates (when using fine-grained deps):
use adk_core::{Content, Event, Part, Agent, Tool, InvocationContext};
use adk_agent::{LlmAgentBuilder, SequentialAgent, LoopAgent, CustomAgentBuilder};
use adk_model::gemini::GeminiModel;
use adk_model::openai::OpenAIClient;
use adk_model::anthropic::AnthropicClient;
use adk_tool::{tool, FunctionTool, McpToolset};
use adk_runner::{Runner, RunnerConfig};
use adk_session::{InMemorySessionService, SessionService, CreateRequest};
use adk_graph::{StateGraph, AgentNode, edge::{START, END, Router}};
```

---

## Agents

### LlmAgent — basic LLM agent

```rust
let model = Arc::new(GeminiModel::new(&api_key, "gemini-2.5-flash")?);
let agent = LlmAgentBuilder::new("agent_name")
    .description("What this agent does")
    .instruction("System prompt / instructions. Supports {template_vars}")
    .model(model)
    .tool(Arc::new(my_tool))           // add one tool
    .tools(Arc::new(my_toolset))        // or add a Toolset
    .build()?;
```

### CustomAgent — arbitrary Rust async fn handler

```rust
let agent = CustomAgentBuilder::new("custom_handler")
    .handler(|ctx: Arc<dyn InvocationContext>| async move {
        let content = ctx.content().unwrap();
        // ... process ...
        Ok(vec![Event::new().with_text("result")])
    })
    .build()?;
```

### SequentialAgent — chain sub-agents in order

```rust
let agent = SequentialAgent::new("pipeline")
    .add_agent(Arc::new(classifier))
    .add_agent(Arc::new(summarizer));
```

### ParallelAgent — run sub-agents concurrently

```rust
let agent = ParallelAgent::new("parallel_workers")
    .add_agent(Arc::new(web_searcher))
    .add_agent(Arc::new(code_analyzer));
```

### LoopAgent — repeat N times

```rust
let agent = LoopAgent::new("retry_loop", 3)  // max 3 iterations
    .add_agent(Arc::new(validator));
```

### LlmConditionalAgent — LLM-based routing

```rust
let agent = LlmConditionalAgentBuilder::new("router")
    .model(model)
    .instruction("Route to 'code' or 'general' based on query")
    .build()?;
```

---

## Tools

### #[tool] macro (preferred)

```rust
use adk_tool::{tool, AdkError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
struct WeatherArgs {
    /// City name to look up
    city: String,
}

/// Get weather for a city.
#[tool]
async fn get_weather(args: WeatherArgs) -> Result<Value, AdkError> {
    Ok(json!({"temp": 22, "condition": "sunny", "city": args.city}))
}
```

### FunctionTool — dynamic/closure-based

```rust
let tool = FunctionTool::new("tool_name", "Description for LLM",
    |ctx: Arc<dyn ToolContext>, args: HashMap<String, Value>| async move {
        // ...
        Ok(json!({"result": "done"}))
    },
)
.with_parameters_schema::<MyArgs>();   // MUST provide schema!
```

### Toolset — group tools

```rust
let mut ts = BasicToolset::new("my_set");
ts.add_tool(Arc::new(tool_a));
ts.add_tool(Arc::new(tool_b));
agent_builder.tools(Arc::new(ts));
```

### Built-in tools

- `GoogleSearchTool` — Gemini web search (model-native)
- `WebSearchTool` — Anthropic Claude web search
- `UrlContextTool` — fetch URL content (Gemini-native)
- `McpToolset::new("name", &[McpServerConfig::new("sid", "cmd", &["args"])])` — MCP servers
- `ExitLoopTool` — signal LoopAgent to stop
- `LoadArtifactsTool` — load stored artifacts by name

---

## Runner + Sessions

```rust
let session_service = Arc::new(InMemorySessionService::new());

let runner = Runner::new(RunnerConfig {
    app_name: "portail".into(),
    agent: Arc::new(agent),
    session_service,                    // Arc<dyn SessionService>
    artifact_service: None,             // Arc<dyn ArtifactService>
    memory_service: None,               // Arc<dyn MemoryService>
    run_config: None,                   // RunConfig
})?;

// Run streaming:
let mut stream = runner.run(
    UserId::new("user_123")?,
    SessionId::new("session_abc")?,
    Content::new("user").with_text("Hello"),
).await?;

while let Some(event) = stream.next().await {
    let event = event?;
    if let Some(content) = event.content() {
        for part in &content.parts {
            if let Part::Text { text } = part {
                print!("{}", text);
            }
        }
    }
}

// Run non-streaming:
let events = runner.run_once(
    UserId::new("user_123")?,
    SessionId::new("session_abc")?,
    Content::new("user").with_text("Hello"),
).await?;
```

### Session management

```rust
let session = session_service.create(CreateRequest {
    app_name: "portail".into(),
    user_id: "user_123".into(),
    session_id: None,           // auto-generated if None
    state: HashMap::from([
        ("user:name".into(), json!("Alice")),
    ]),
}).await?;

// State supports template vars: instruction "Hello {user:name}!"
```

---

## Graph Agents (adk-graph)

```rust
use adk_graph::{StateGraph, AgentNode, edge::{START, END, Router}};

let graph = StateGraph::with_channels(&["input", "sentiment", "output"])
    .add_node(classifier_node)   // AgentNode
    .add_node(response_node)
    .add_edge(START, "classifier")
    .add_conditional_edges("classifier",
        Router::by_field("sentiment"),
        [("positive", "response"), ("negative", "response")],
    )
    .add_edge("response", END);

// Wrap graph as ADK Agent:
let graph_agent = GraphAgent::new("workflow", Arc::new(compiled_graph));
```

### AgentNode with mappers

```rust
let node = AgentNode::new(agent_arc)
    // Map state to agent input Content
    .with_input_mapper(|state| {
        let text = state.get("input").and_then(|v| v.as_str()).unwrap_or("");
        Content::new("user").with_text(text)
    })
    // Map agent output events back to state
    .with_output_mapper(|events| {
        let mut updates = HashMap::new();
        // ... extract values from events ...
        updates.insert("result".into(), json!(...));
        updates
    });
```

---

## Callbacks / Lifecycle Hooks

ADK-Rust does NOT have explicit callback/hook traits on LlmAgent.
Instead, use wrapping patterns:

```rust
// Wrap an agent with a CustomAgent that adds instrumentation:
let inner = Arc::new(my_llm_agent);
let wrapped = CustomAgentBuilder::new("wrapped")
    .handler(move |ctx| {
        let inner = inner.clone();
        async move {
            tracing::info!("before agent");
            let result = inner.run(ctx).await;
            tracing::info!("after agent");
            result
        }
    })
    .build()?;
```

Or use `Runner` with custom `RunConfig`.

---

## Guardrails

```rust
use adk_guardrail::{Guardrail, GuardrailConfig};

// Guardrails are NOT in the prelude — import from adk-guardrail.
// Pattern: wrap an agent with input/output guardrail checks.
```

---

## Configuration via RunConfig

```rust
let run_config = RunConfigBuilder::default()
    .max_iterations(10)
    .max_time(30.0)     // seconds
    .build()?;

Runner::new(RunnerConfig {
    run_config: Some(run_config),
    // ...
})?;
```

---

## Error Handling

```rust
use adk_core::AdkError;

// All tool functions return Result<Value, AdkError>
// AdkError has: AdkError::ToolExecution(msg)
//               AdkError::ModelError(msg)
//               AdkError::SessionError(msg)
//               AdkError::ConfigurationError(msg)
//               AdkError::InvalidRequest(msg)
```

---

## Common Patterns

### Agent with MCP tools
```rust
let mcp = McpToolset::new("my_mcps", &[
    McpServerConfig::new("fs", "npx", &["-y", "@modelcontextprotocol/server-filesystem", "."]),
]);
agent_builder.tools(Arc::new(mcp));
```

### Agent with search
```rust
// Gemini: GoogleSearchTool is built-in, no config needed
// Anthropic: WebSearchTool is built-in, no config needed
```

### Custom model provider
```rust
// Implement the Llm trait + provide via adk-model's provider enum.
// See adk_model::openai::OpenAIClient for reference.
```

### Non-streaming one-shot
```rust
let events = runner.run_once(user_id, session_id, content).await?;
for event in &events {
    if let Some(text) = event.content().and_then(|c| c.text()) {
        println!("{}", text);
    }
}
```

---

## Running with Launcher (CLI mode)

```rust
use adk_rust::Launcher;

Launcher::new(Arc::new(agent)).run().await?;
// Launcher handles stdin/stdout interactive loop automatically.
```
