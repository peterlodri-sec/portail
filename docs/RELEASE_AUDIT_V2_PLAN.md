# Release Audit v2 — Reverse Engineering Deep-Audit Pipeline

**Status:** Planned (v2.6)

## v1 (current — `portail release-audit`)

Minimal deterministic audit:
- Binary file type detection (ELF/Mach-O/PE)
- Architecture detection
- Strip check (ELF debug sections)
- SHA256 hashing + manifest
- Suspicious string scanning (build paths, credentials, debug refs)
- CycloneDX SBOM generation (`release-audit-sbom.cdx.json`)
- Markdown audit report (`release-audit-report.md`)
- Runs as a CI step in `release.yml` after builds, before GitHub Release

## v2 — RE Deep-Audit

E2E pipeline on **devcx53** (isolated reverse-engineering playground):

```
Release artifacts
    │
    ▼
devcx53 (isolated RE playground)
    │
    ├── RE-agent-fleet (multi-agent binary analysis)
    │   ├── Binary extraction & unpacking
    │   ├── Control-flow graph reconstruction
    │   ├── Symbol recovery & deobfuscation
    │   └── Vulnerability pattern matching
    │
    ├── Ghidra (SRE framework)
    │   ├── Automated disassembly & decompilation
    │   ├── Function signature recovery
    │   ├── Type reconstruction
    │   └── P-code analysis
    │
    ├── Ghidra MCP (MCP bridge for Ghidra)
    │   ├── Headless Ghidra project management
    │   ├── Script execution & analysis dispatch
    │   └── JSON-RPC API over stdio/Unix socket
    │
    └── Output
        ├── Deep RE report (structured JSON + Markdown)
        ├── Ghidra project archive (.gpr)
        ├── Decompilation listings
        └── Vulnerability findings
```

### Key differences from v1

| Aspect | v1 (current) | v2 (planned) |
|--------|-------------|--------------|
| Analysis depth | Surface-level (magic bytes, arch, strings) | Deep (CFG, decompilation, types) |
| Tooling | stdlib + `strings` | Ghidra + Ghidra MCP + RE-agent-fleet |
| Infrastructure | CI runner | Isolated devcx53 playground |
| AI/LLM usage | Zero | RE-agent-fleet (multi-agent orchestration) |
| Output | SBOM + manifest + report | Full RE report + Ghidra project + findings |

### Infrastructure

- **devcx53** — isolated VM/container for RE tooling
  - Ghidra headless installed + configured
  - RE-agent-fleet deployed as sidecar
  - Ghidra MCP bridge exposed via Unix socket
  - No network access to production systems
- Pipeline triggered from `release.yml` via SSH/deploy key
- Results uploaded back as release artifacts

### Integration

```yaml
# Future release.yml step
- name: Run RE deep-audit (devcx53)
  run: |
    ssh devcx53 "portail release-audit-re \
      --input dist/ \
      --ghidra-project /var/portail/ghidra/ \
      --agent-fleet-endpoint unix:///var/run/re-agent.sock"
```
