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
The stream carries only opaque invalidation metadata. On a server invalidation,
Web serializes `sync_now()` before reading Core state so the server's event-log
tail is applied locally; its refresh controller coalesces bursts and allows only
one sync/refresh sequence at a time. Mobile clients refresh canonical state
through Core after receiving a signal because their normal socket synchronization
path already applies remote events. Web uses an authenticated fetch stream
because native `EventSource` cannot send an `Authorization` header;
mobile clients attach the same Bearer credential to their Ktor stream and obtain
a new credential on reconnect.

CLI recovery-approval commands also flush locally queued decisions before they
report success, so a completed command means the server has received the decision.
After `meta-cli secret show --claim-id` reconstructs a Secret, it asks the
Core SyncGateway to complete that exact recovery claim before printing success.
Core derives the unique approved receiver and Pass ID from the persisted claim,
signs `SsRecoveryCompletion`, and requires a successful server response. Local
`Delivered` state alone does not finish recovery or notify the other members.

Pending JOIN devices use a distinct `/join-events` stream because they are not
Vault members yet. A Core-generated UUID connects one JOIN request to its exact
membership decision. Core signs a short-lived credential containing the
request UUID and candidate public key. The server verifies signature and
expiry before reading Vault state, then checks the credential key against the
canonical candidate and the exact pending request or compact terminal record
in VaultLog. Terminal records survive cleanup of transient updates and older
VaultLogs deserialize with an empty record set. The
stream emits only the matching request UUID and accepted/declined outcome;
clients refresh canonical state once after the event. Web sends the token in an
`Authorization` header with fetch streaming, never in the URL. Legacy JOIN
events with no UUID fail closed for this stream, with no polling fallback.

For local visible E2E only, the server may attach a `recovery_decision` signal
to the authenticated `/state-events` stream. The runner enables this explicitly
with `META_SERVER_E2E_SIGNALS=1`; it is disabled by default. The signal carries
only the exact recovery `claimId` and accepted/declined outcome, emitted after
the server persists the corresponding claim state. It contains no Secret,
Master Key, or Key Share data. The Web test matches the signal to the claim it
approved, then checks the rendered request state. Time limits remain failure
watchdogs, not synchronization boundaries.

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
