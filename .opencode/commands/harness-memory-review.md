---
description: Review and promote/reject candidate memories
---

Run `npx harness-memory memory:list --db .harness-memory/memory.sqlite --status candidate` to show candidates.

For each candidate, ask the user to approve or reject:
- Approve: `npx harness-memory memory:promote --db .harness-memory/memory.sqlite --memory <id>`
- Reject: `npx harness-memory memory:reject --db .harness-memory/memory.sqlite --memory <id>`
