# AI Architecture — meta-secret-core

Single source of truth for AI automation in `meta-secret-core`.

## Structure

```text
.ai/
├── README.md
├── INDEX.md
├── QUICK-START.md
├── WORKFLOW.md
├── PIPELINE.md
├── ORCHESTRATOR.md
├── ARCHITECTURE.md
├── agents/
├── commands/
├── skills/
├── rules/
└── artifacts/
    ├── *-template.md
    └── run/
```

## Stage Model (core)

Core follows an 8-stage workflow:

1. Issue Intake
2. Planning
3. Implementation
4. Build
5. Code Review
6. Test Authoring
7. Test Run
8. Branch + Commit + PR

This is intentionally different from compose (no UI/design review split), because core is Rust-first and FFI-sensitive.

## Language and Architecture Adaptation

- Primary stack: Rust workspace in `meta-secret/`
- Build/test contract: Cargo-centric (`cargo build`, `cargo test`)
- FFI contract: UniFFI surface must be treated as compatibility boundary
- Cross-repo constraint: FFI/API changes must call out impact on `meta-secret-compose`

## Artifacts Contract

- Directory: `.ai/artifacts/run/`
- Naming: `MS-<run-id>-<stage-number>-<stage-name>.md`
- Status fields required for gate stages:
  - `Status: PASSED / FAILED`
  - `Return to Planning: YES / NO`

## Commands and Agents

- `commands/` keeps user entry points (`only-*`, `workflow-from-*`)
- `agents/` keeps stage ownership (`feature-planner`, `code-implementer`, `code-reviewer`, etc.)
- `WORKFLOW.md` and `PIPELINE.md` are the only canonical stage definitions; other files must reference them.

## Maintenance Rules

- Do not duplicate stage logic in IDE-specific entry files.
- Keep command descriptions aligned with actual stage model and artifacts.
- Update templates and stage docs together when pipeline changes.

Last updated: 2026-04-22
