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

Core follows `.ai/WORKFLOW.md`, including constraint validation, TDD, build,
security/code/design reviews, coverage reporting, regular tests, final applicable
UI E2E and action-specific release authorization. The UI migration applicability
profile in `.ai/rules/ui-migration-profile.md` selects client evidence for Web-only
changes; `.ai/rules/coverage-policy.md` makes overall percentage goals advisory.
Do not maintain a shortened alternate stage sequence here.

Core repository also owns Vue Web UI under `meta-secret/web-cli/ui/`. Web UI
changes do not themselves imply Rust protocol/FFI changes. Domain and security
logic remains Rust-owned; browser components render typed presentation state.

## Authenticated Sync Boundary

All state-changing writes and recovery completion commands cross the sync boundary
as Core-owned `SignedAction` values. The envelope contains a deterministic
canonical payload, signer Device ID, action stream, monotonic nonce, and Ed25519
signature. The server resolves the signer's public key from canonical Vault
membership, checks authorization and sequence freshness, then applies the event.
Unsigned, modified, unauthorized, and replayed commands are rejected before
persistence. Device removal uses the existing Core-owned SignedAction transport and role checks, including local signed confirmation for 2→1 removal.

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

### Web recovered Show lifetime

The Web Liveboard parent owns one manager, authenticated SSE and serialized Store
sync/read lane. An explicit visible recovered Show borrows the exact owned Claim
for one SDK call and frees it once in finally. Its raw return is not a persisted
acknowledgement. One private unfinished capsule may await fresh canonical proof;
it is never a finished-value cache, UI/store value or persisted/logged payload.

Before intentionally retiring the no-replay subscription, the parent synchronously
latches its required barrier and conceals ALL held plaintext, Copy and motion.
Actual successful authenticated HTTP response/body acknowledges the replacement
receiver after server subscribe; there is no initial ready frame or replay. A
separate metadata-only parent gap obligation advances a NEW required physical
sync/read target after headers, behind pre-ready flights in the same lane. It
survives cancellation of the pending value by route/filter/Hide/modal. Ineligible
hidden/offline work stays blocked until normal eligible required refresh; old
identity/key/manager/generation/disposed work cannot synchronize a successor.

First publication still requires the original one-call return and live exact local
owner, current parent/lease/ready epoch, exact fresh accepted Done for the same
Claim/Pass/requester, Core-ready Recover and unchanged relevant canonical history.
Unexpected replacement/deletion/history/actor/member transitions revoke, without
client quorum or eligibility inference. Decode/delivery is synchronous once after
the new fence; only true delivery consumes exact unread. Every hard privacy/error
boundary drops capsule/held values; no reconnect or readiness restores them.
Persistent parent stream-loss ownership remains after temporary delivery-listener
cleanup: current EOF/auth/transport/missing-body conceals and blocks even after
publication. No extra Show/auth/retry, plaintext persistence or logging is added.

See [Web Show ownership](../docs/liveboard-bindings.md#web-recovered-show-delivery).
For run MS-20261019210543 the accepted Recovery source handoff has222 passing
composed client/Store/SSE checks; real Core Stage9 Recover continuation and final
original UI E2E remain
separate pending gates, not implied by that source contract or component evidence.

### Web manual04 error ownership

Every explicit controller error report is a synchronous privacy event, including
an identical sanitized message still visible in tile04. A structural setter scope
prevents only duplicate watcher handling; internal notification errors retain the
watcher fallback. The current controller/revision hook revokes unfinished recovered
capsules and latches the existing required barrier, clearing held values, Copy,
motion. Authored Create input has separate reason-based ownership: ordinary SAME-session
sync preserves its open state/full Plain12/24 input, while errors close the form but
retain that SAME input for deliberate reopen. Actual hard boundaries and idle Cancel
clear every field. The sessionError helper carries metadata/callbacks only.

A manual04 tap clears display and requests the same parent's existing required
sync_now→get_state lane when active. Dismissal, cached/older reads and time do not
prove readiness; failed sync remains blocked. Normal genuine sync may restore
technical readiness while the same-parent04 error stays pinned until tap.
Hidden/offline owners retain required work for normal eligible resume; existing
auth/key/manager/generation/revision/member/Reset/disposal fences protect successors.
No second lane, SSE, polling, automatic Show/Copy/business retry or value restoration
is introduced. Exact queued requests, pending Unknown and original R/U stay intact.
See [Web manual error/read contract](../docs/liveboard-bindings.md#web-manual04-errors-and-required-reads).

The parent blocks reentrant Create admission before observable error/sync/projection
effects. Per-write tickets and exact receipt ownership protect the outstanding write
across soft/hard view conceal; exact terminal consumption precedes observer effects,
including while its form is closed. Input edit/clipboard revision is separate from
native-result lifetime; no new Core permission, retry, lane or Web Create auth exists.
Current bounded client validation is58 selected/665 full tests,63-module standard
coverage95.8% lines/89.69% branches/88.4% functions, typecheck/lint/build PASS.
[Final independent review](artifacts/run/MS-20261019210543-005c-web-create-parent-final-review.md)
binds source, explicit two obsolete-oracle amendments and preserved harness histories.
Postcallback guards include source-reviewed properties beyond direct probes. Real
browser/Core UI E2E, native renderer/OS and normal-motion fidelity are separate gates.

## Technical DeviceLog transport

The separately owner-approved technical DeviceLog Noop uses its own signed
purpose and dedicated write verifier under the same server request owner. It
occupies only a current member's own next empty transport slot, never applies a
Business command, and never becomes original Business raw acknowledgement.
Typed transport readers continue across it during mandatory ciphertext purge;
typed public canonical/history/fallback and complete addressed-workflow validation
precede delivery effects. Server layers A/B are implemented. Client C1b stores
complete private all-Pass preparation, verifies the changed fixed-index revision
and reads back exact origin before body under the shared append owner. C1b has
independent source acceptance. The bounded initial C2 path durably freezes the
original compact signature before send and publishes the same frozen material
only after exact locally imported own Applied proof/current context. Actual claim
IDs, ordinary handoff and private duplicate scrub use readbacks under the same
sync owner. Selected C2/C3 paths have bounded independent/component evidence;
healthy explicit rollover additionally reaches real SQLite reopen and server
admission, with its final source review and later negative/fault groups separate.
Only an explicit checked action can roll a settled blocked N into FULL Staged:
current-user technical verification → complete ephemeral compute → changed
EXISTING-index readback → atomic same-key FULL/compact-N readback → brief append.
Reopened FULL retry verifies retained history before append without recomputing.
If CURRENT technical metadata is supplied, verify it against CURRENT origin with
the same actual-user verifier before append. Preserve its optional partial proof
semantics; settled HISTORY still requires both receipt and confirmed tail.
Selected other-request winner retirement requires intact Pending/FULL/current
origin, no result of any phase for the losing UUID, exact surviving owner/two
members and the captured target now Declined. A different Accepted/Applied
result must bind the reversed roles and exact responder source. Existing
auth/readback/purge/Noop/receipt/tail rules then apply; active local inputs and
exact own Applied publication remain protected. The first C4 incoming host case now
imports the actual returned current K2 despite retired prospective K1 at the same
physical key, preserving exact current/previous ownership exclusion and stronger
applicable published-own proof. Repeat and two real Member3 GetState calls retain
the technical compact. The selected outgoing metadata/current-raw/history lookup,
Member2 finalization and later canonical Join lifetime now also have bounded host
evidence. Raw fallback coverage proves absent ciphertext/zero replay, not a
positive fallback upload. Native package/UI acceptance remains separate.
Bounded service Reset now checks the same persisted Declined session and existing
removal evidence before propagating an irrelevant sync/finalizer error; unproven
cases retain the original error. Public GetState and direct recovery writers
share the client owner, and exact revalidation/cleanup also owns sync. Shared
recovery helpers check the persisted full session and retain client ownership
through local completion. Host tests reach cleanup and writer ordering. The mobile
manager's failed pre-sync now uses one Rust-only terminal handoff to the same proof;
only actual proven cleanup returns Local, otherwise the original error is retained.
Its successful two-pass/FIFO path is unchanged. Actual-manager host tests cover both
paths; native packages/UI and global/manual Reset remain separate.
The same shared sync owner spans the reads/compute/store interval; no recursive
sync/client lock or append owner across compute/probe is introduced. Noop remains
technical, not Applied/readiness. An exact original own Applied source is distinct
from current publication applicability: proven later Join chronology terminates
the old lifetime with a read-back Blocked/None record before Join compute, while
preserving every compact proof. An already Published/None lifetime can also end
after an exact later canonical A+B→A removal: contiguous typed public history,
unique full Pending/Prepared/Applied lineage, unchanged A+B ancestry and the exact
own raw R/U pair are required. Both selected UUIDs retain their immutable result
fields/source and agreeing Prepared receipts cumulatively; only Prepared→Applied
is legal, with no disappearance, regression or later Pending. The ordinary later
source=None is valid; this is
canonical content authority, not an invented signed-envelope digest proof. A
present producer context must agree, while a legitimately cleared context grants
nothing and is not required permanently. Existing-only terminal replacement and
whole-object readback preserve every compact field and remain monotonic. Exact
terminal input may use the actual canonical Split empty receiver/status profile
only with stable identity, the same committed own WholeCopy and full persisted
local membership. This is ordinary Join input compatibility across later accepted
Joins/reopen, not old publication or Reveal authority; nonterminal receivers stay
exact. A Published/None canonical per-Pass tombstone
keeps exact sequential ownership metadata and proves raw absence; other active
Passes retain all strict ordinary proofs. No whole-witness tombstone retirement or
selector rewrite is used. Full Core build/review/coverage and client/package/UI
handoff gates remain separate. See
[transport contract and compatibility](../docs/device-transport-noop.md) and
[private preparation persistence](../docs/guarded-preparation.md).

## Private Recovery response durability

The Liveboard migration adds one narrowly scoped server-private Recovery response
reservation per Vault. SQLite (the current HTTP backend), ReDB and InMem own a
separate private store outside GenericKvLogEvent/ArtifactId storage. Actual
ServerApp startup requires its readiness. Under the existing request owner,
Recovery repair/verified cleanup precedes Removal repair and ordinary
retirement/purge/read; discovery includes the actual Vaults addressed by generic
read tails. Only compact optional source proof extends canonical winner metadata;
signed action/claim/completion shapes and requester completion roles remain.
Exact authenticated source acknowledgement does not reapply a command or
resurrect consumed ciphertext. See [server persistence contract](../docs/server-recovery-persistence.md)
for phase ordering, compatibility, backend lifecycle and privacy boundaries.

## Local client service ownership

MetaClientService owns one bounded FIFO of local request envelopes. Each waiting
call retains its own bounded reply receiver; its move-only reply sender travels
with exactly that request. Checked and legacy business variants use the same
handler/executor and existing operation guard. Fire-and-forget requests have no
reply target. These transient transport objects never enter serialization,
network/FFI schemas, operation metadata, persistence or payload-bearing logs.

The run claims the sole receiver before startup and closes/drains it on exit.
A second run cannot share it. A dropped caller does not cancel or resubmit an
already admitted command; the eventual reply is safely discarded. Startup/run
failure releases pending calls without assigning another call's result. A closed
transfer requires normal fresh application initialization. Pre-run diagnostic
drain/requeue shares the claim/close ownership lock, never waits for capacity and
drops the original target on failure. See [binding contract](../docs/liveboard-bindings.md).

## Local producer completion versus authorization

Additive Delete/Remove producer requests use the same local FIFO, private reply
and operation owner. Actual R/U references and local material stages persist in
one optional compact field of the existing local-only removal registry; guarded
selector entries retain their own meaning. Read-only reconcile makes one atomic
ServerTail observation and never synchronizes, signs/appends, finalizes material
or changes this context.

Explicit same-U authorization while two members remain requires the exact
captured admission Vault and existing role policy. Already-Applied singleton
completion is separate: exact original bodies, fresh BOTH-source raw ACKs and
Applied UUID, unique matching canonical result, current surviving actor and
actual local material/readiness permit ONLY verified same-context metadata
update/clear. Stored stages and an old Applied observation cannot authorize
cleanup. No sync, new body/signature or new material is performed. Whole-copy readiness reuses Core's shared non-revealing
local material loader; current1/2 authorization permits stored n1/n2 with k1,
independent of current N and mutable Split administrative owner. Projection keeps
exact canonical material binding, while old public Show adds no canonical claim
or history requirement. Current3/recovery retains exact k2 validation. No
reconstruction, material mutation or new generation occurs during availability. See [binding contract](../docs/liveboard-bindings.md).

## Language and Architecture Adaptation

New Core-shaped initial Recover intake uses canonical claim + temporary
metadata receipt in one fixed SsLog save, original raw source readback, then
atomic cleanup in that same target. Whole affected history/addressed-record
audits precede any repair or effects; non-head/conflicting Prepared records fail
closed. Immutable original receivers are independent of topology changes before
admission; the receipt anchors the actual admission Vault and reopen still
requires that exact captured context. It adds no command, replay allowance or private table and does not heal
legacy proofless orphans. See [initial request durability](../docs/server-recovery-request-durability.md)
for admission exclusions, ordering, backend/scan costs and downgrade limits.

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
