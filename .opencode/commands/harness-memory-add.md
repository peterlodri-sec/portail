---
description: Add a new project memory
---

Ask the user what they want to remember. Then run:
```bash
npx harness-memory memory:add --db .harness-memory/memory.sqlite --type <type> --summary "<summary>" --details "<details>"
```

Valid types: policy, workflow, pitfall, architecture_constraint, decision
The memory is created as a candidate. Promote it with /harness-memory-review.
