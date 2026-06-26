# Portail Philosophy

## Principles

### 1. Two providers, everything else is a plugin

OpenRouter and DeepSeek are the only natively supported LLM providers.
OpenRouter is the universal key — one abstraction for the agent SDKs,
one auth path, one code path. Every other provider belongs behind the
plugin/hook system. Two perfect abstractions beat twenty half-baked ones.

### 2. No crypto, no blockchain

There will never be an official plugin, connector, or service for
cryptocurrency or blockchain inside Portail. The technology itself
(immutable ledgers, zero-knowledge proofs) is TBD and evaluated on
technical merit alone. This project is and will always be open source
first. If a corporate entity ever needs an enterprise edition, the
current maintainers and contributors vote as a council.

### 3. Driven by one person's needs, open to all

The main direction comes from the problems I solve for myself. That
does not make it closed-opinion. The vision is documented in this
repository and anyone is welcome to contribute, fork, or build on it.
The core trajectory is set; the ecosystem is open.

### 4. Telemetry is on by default — transparent, anonymous, opt-out

Portail is a live research project with a side goal of collecting
high-quality dogfood data. SOTA anonymous, non-tracking telemetry
is **on by default**. You can inspect exactly what is collected
(follow the OpenTelemetry protocol implementation and the published
audit). Only anonymous usage data is collected — no prompts, no
responses, no identifiable information. The interactive setup wizard
asks for double confirmation. If you're not in a corporate environment,
leave it on — it directly improves the optimization loops the project
exists to run. If you need it off, toggle it. No pressure, but thanks
if you help.

### 5. The output is intelligence, not code

Code is incidental. The value is reasoning — composite intelligence
built from small, focused components. We use Rust for the substrate
and LLMs for the judgment. Every line of code earns its place.

### 6. Advisory agents, never gates

CI agents report, recommend, annotate — they never block a merge.
Knowledge is additive; process is subtractive. We trust contributors
to read a comment and decide.

### 7. Council decisions, not dictatorships

Ship / Iterate / Escalate. The loop always has a human out. No
autonomous system in this project makes an irreversible decision
without a verified human in the loop.
