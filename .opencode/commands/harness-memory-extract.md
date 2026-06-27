---
description: Extract memories from recent conversations (LLM-based)
---

Run `npx harness-memory dream:extract --db .harness-memory/memory.sqlite --skip-gates` to analyze buffered conversations and extract memory-worthy facts.

This uses the LLM to identify preferences, decisions, and constraints from your recent conversations.

After extraction, run /harness-memory-review to approve the candidates.
