# Loop Engineer CI Agents

Minimal, composable CI agents for automated code quality loops.

## Agent Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Loop Engineer CI Agents                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   ┌───────────┐  ┌───────────┐  ┌───────────┐              │
│   │  Scout    │  │  Critic   │  │  Fixer    │              │
│   │  (find)   │  │  (review) │  │  (patch)  │              │
│   └─────┬─────┘  └─────┬─────┘  └─────┬─────┘              │
│         │              │              │                     │
│         └──────────────┼──────────────┘                     │
│                        ▼                                    │
│              ┌───────────────────┐                          │
│              │  Loop Controller  │                          │
│              │  (iterate/correct)│                          │
│              └───────────────────┘                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Agents

### 1. Scout Agent (Find)

**Purpose**: Scan codebase for issues, opportunities, patterns.

**Input**: Repository path, ruleset
**Output**: List of findings with severity, location, suggestion

**Loop**: 
- Run on every PR
- Find: unused imports, dead code, complexity hotspots, missing docs
- Output: JSON report

**Implementation**: `scripts/loop-scout.sh`

```bash
#!/bin/bash
# Scout agent: find issues in codebase
cargo clippy -- -D warnings 2>&1 | parse_clippy
cargo test 2>&1 | parse_tests
grep -rn "TODO\|FIXME\|HACK" src/ | parse_todos
```

### 2. Critic Agent (Review)

**Purpose**: Review code changes for quality, security, performance.

**Input**: Diff, context, ruleset
**Output**: Review comments with severity, line number, suggestion

**Loop**:
- Run on every PR
- Review: security issues, performance concerns, style violations
- Output: GitHub PR comments

**Implementation**: `scripts/loop-critic.sh`

```bash
#!/bin/bash
# Critic agent: review changes
git diff main..HEAD | analyze_diff
cargo audit 2>&1 | parse_audit
```

### 3. Fixer Agent (Patch)

**Purpose**: Automatically fix issues found by Scout/Critic.

**Input**: Findings list
**Output**: Patches applied, PR created

**Loop**:
- Run when Scout/Critic find issues
- Fix: formatting, imports, simple refactors
- Output: PR with fixes

**Implementation**: `scripts/loop-fixer.sh`

```bash
#!/bin/bash
# Fixer agent: apply automatic fixes
cargo fmt
cargo fix --allow-dirty
cargo clippy --fix --allow-dirty
```

### 4. Complexity Agent (Analyze)

**Purpose**: Track code complexity over time.

**Input**: Source code
**Output**: Complexity report, trends

**Loop**:
- Run on every push to main
- Analyze: cyclomatic complexity, cognitive complexity, O(n) annotations
- Output: Markdown report, trend graph

**Implementation**: `scripts/complexity-analysis.sh` (exists)

### 5. Security Agent (Audit)

**Purpose**: Continuous security scanning.

**Input**: Dependencies, code, config
**Output**: Vulnerability report, remediation steps

**Loop**:
- Run daily + on every PR
- Scan: cargo audit, dependency CVEs, secret detection
- Output: Security report

**Implementation**: `scripts/loop-security.sh`

```bash
#!/bin/bash
# Security agent: continuous audit
cargo audit 2>&1
cargo deny check 2>&1
grep -rn "password\|secret\|key" src/ | grep -v "test"
```

### 6. Performance Agent (Benchmark)

**Purpose**: Track performance regressions.

**Input**: Benchmarks, baseline
**Output**: Performance report, regression alerts

**Loop**:
- Run on every PR
- Benchmark: hot paths, allocations, latency
- Output: Comparison with baseline

**Implementation**: `scripts/loop-perf.sh`

```bash
#!/bin/bash
# Performance agent: benchmark and compare
cargo bench 2>&1
compare_with_baseline
```

### 7. Docs Agent (Document)

**Purpose**: Ensure documentation is complete and up-to-date.

**Input**: Source code, existing docs
**Output**: Missing docs, outdated docs

**Loop**:
- Run weekly + on API changes
- Check: public API docs, README accuracy, example correctness
- Output: Documentation gaps

**Implementation**: `scripts/loop-docs.sh`

```bash
#!/bin/bash
# Docs agent: check documentation
cargo doc --no-deps 2>&1
check_public_apis_documented
```

### 8. Loop Controller (Orchestrate)

**Purpose**: Run agents in sequence, handle feedback loops.

**Input**: PR, context
**Output**: Final decision (approve/request changes)

**Loop**:
- Run all agents
- If issues found → Fixer → re-run agents
- Max 3 iterations
- Output: Final verdict

**Implementation**: `scripts/loop-controller.sh`

```bash
#!/bin/bash
# Loop controller: orchestrate agents
MAX_ITERATIONS=3
for i in $(seq 1 $MAX_ITERATIONS); do
    ./scripts/loop-scout.sh
    ./scripts/loop-critic.sh
    if no_issues; then
        echo "✓ All checks passed"
        exit 0
    fi
    ./scripts/loop-fixer.sh
done
echo "✗ Issues remain after $MAX_ITERATIONS iterations"
exit 1
```

## GitHub Actions Integration

```yaml
name: Loop Engineer

on: [pull_request]

jobs:
  scout:
    runs-on: self-hosted
    steps:
      - uses: actions/checkout@v7
      - run: ./scripts/loop-scout.sh
  
  critic:
    runs-on: self-hosted
    steps:
      - uses: actions/checkout@v7
      - run: ./scripts/loop-critic.sh
  
  fixer:
    needs: [scout, critic]
    if: failure()
    runs-on: self-hosted
    steps:
      - uses: actions/checkout@v7
      - run: ./scripts/loop-fixer.sh
```

## Benefits

1. **Automated quality** — Issues caught before human review
2. **Consistent standards** — Same rules applied to every PR
3. **Fast feedback** — Developers get instant feedback
4. **Self-healing** — Simple issues fixed automatically
5. **Auditable** — All decisions logged and traceable
