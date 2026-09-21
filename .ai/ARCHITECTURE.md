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

Core follows the stage workflow defined in `.ai/WORKFLOW.md` (including the
conditional documentation, UI-E2E, and approval gates):

1. Issue Intake
2. Planning
3. Implementation
4. Build
5. Code Review
6. Test Authoring
7. Test Run
8. Branch + Commit + PR

Core is Rust-first and FFI-sensitive; server-authentication changes are tested at
the protocol/integration layer, while visual UI E2E is used only when a client
contract visibly changes.

## Authenticated Sync Boundary

All state-changing writes and recovery completion commands cross the sync boundary
as Core-owned `SignedAction` values. The envelope contains a deterministic
canonical payload, signer Device ID, action stream, monotonic nonce, and Ed25519
signature. The server resolves the signer's public key from canonical Vault
membership, checks authorization and sequence freshness, then applies the event.
Unsigned, modified, unauthorized, and replayed commands are rejected before
persistence. DELETE DEVICE is intentionally outside this issue's protocol scope.

State invalidation uses a separate authenticated signal boundary. Core issues a
short-lived `StateEventsSubscription` signed by the device DSA key; the server
checks the signature and current Vault membership before opening `/state-events`.
The stream carries only opaque invalidation metadata, and every client refreshes
canonical state through Core after receiving a signal. Web uses an authenticated
fetch stream because native `EventSource` cannot send an `Authorization` header;
mobile clients attach the same Bearer credential to their Ktor stream and obtain
a new credential on reconnect.

## Language and Architecture Adaptation

- Primary stack: Rust workspace in `meta-secret/`
- Build/test contract: **`task`** from repo root for CI parity (see [`.ai/skills/build-via-task/SKILL.md`](skills/build-via-task/SKILL.md)); narrow `cargo build` / `cargo test` from `meta-secret/` for single-crate iteration
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
