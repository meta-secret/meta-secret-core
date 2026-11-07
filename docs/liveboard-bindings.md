# Additive Liveboard bindings

These APIs are additive to the legacy WASM and UniFFI exports. A client uses the Core-owned `liveboardState`
snapshot for eligibility and operation progress. This document describes the source contract; generated
browser/native packages must be rebuilt and validated before client adoption.

## Native Recover approval failure evidence

The native `accept_recover(opaque_claim_id) -> String` retains its existing
success/error fields and ABI. A false envelope may additionally contain:

```json
{"decisionAttempt":{"schemaVersion":1,"kind":"recoverApprove","claimId":"opaque-claim","actorId":"device-id","vaultName":"vault-name","phase":"notSubmitted"}}
```

This evidence describes only the completed invocation's first pre-sync failure
before response production, plus a locked audit of its exact local actor, Vault,
claim and response slots. It is not proof of historical server absence, an
acknowledgement, permission or a reusable retry token. Existing queued Recovery
or Decline, unreadable/malformed source, incomplete or nonpending receiver/status
binding, foreign identity and unproved state omit it. Every failure after the
initial pre-sync—including response persistence and post-sync—also omits it.
The original failure remains a failure.

The native typed path preserves the original pre-sync, queued GetState/FIFO and
state publication. Before response creation it adds a fresh canonical read under
the existing native service's shared client owner; current permission, exact
identity and source validation, response creation and post-sync remain in that
owner interval. Local audits take the existing device-sync guard in client→sync
order and release it before synchronization. This guarantee applies to the same
native service/PersistentObject, not independent wrappers, processes or the
separate virtual-device executor.

A consuming client must bind validated evidence to the immutable original
completed intent and same native owner/scope, require a successful post-return
canonical read, then a new explicit action and fresh authentication. Missing,
unknown, malformed or foreign evidence grants nothing. A terminal request needs
no second decision. Disabled decision controls with unknown resolution, null
outcome and no verified responder are not a terminal proof for an uncertain
intent; legacy Decline projections may have this conservative shape.
Dismissing a visible error never submits a decision or turns
an uncertain result into success. Client decoder compatibility and native binary
qualification are separate required gates; unchanged String ABI alone does not
establish them.

## Checked operations and exact decisions

`WasmUserMemberFullInfo.recovery_claim_by_id(opaque_id)` resolves an exact Recover key from that snapshot and
returns a new owned `ClaimId`. Input must be nonempty and at most 512 UTF-8 bytes; the map key, claim ID,
distribution claim ID and Vault must agree. Unknown or inconsistent identity returns None. It does not choose the
first claim for a Secret, synchronize, change status or authorize an action. Existing Core action ports still
validate the exact current claim and actor; native exact-string methods are unchanged. The owned result can outlive
its snapshot. Generated WASM borrow/free validation remains a separate packaging gate.

| Operation | WASM method | UniFFI/common JSON method |
| --- | --- | --- |
| Create | `cluster_distribution_checked(&PlainPassInfo)` | `cluster_distribution_checked(name, secret)` |
| Recover | `recover_checked(&MetaPasswordId)` | `recover_checked(secret_id)` |
| Join decision | `decide_join_request(request_json)` | `decide_join_request(request_json)` |
| Remove decision | `decide_remove_member(request_json)` | `decide_remove_member(request_json)` |

The generic managers enqueue one typed request through the existing serial Core service. They do not call a
separate distribution executor, detect whether the service is running, or generate replacement operation IDs.
Native Recover resolves the supplied Secret identifier through the existing Core resolver; the resulting actual
Pass identity is passed to the checked operation.

The local service uses one bounded FIFO and a private one-response channel for each waiting call. Checked and
legacy requests share the same executor and operation guard. Correlation is an internal move-only reply target; it
is never serialized, logged, persisted or passed through network/FFI payloads.

Canceling a caller before its request enters the queue performs no operation. After admission, its command may
finish once, but its abandoned response cannot reach another caller or block the executor. Core does not
automatically retry or resubmit it. The run owns the sole receiver before startup; startup failure or run shutdown
disconnects active/queued callers and permanently closes that local transfer. Application reinitialization creates
a fresh service/transfer.

Diagnostic drain/requeue is allowed only before the run claims the receiver. It retains each original reply target
and uses nonblocking admission; failure drops that target. Requeue takes a new FIFO position. Relative batch order
is preserved only when no other producer interleaves the inspection and requeue.

Both decision APIs accept exactly this JSON object:

```json
{"requestId":"d732b6ac-0881-42ac-abd6-a5cba32b34ec","accepted":true}
```

The shared Core parser accepts at most 1024 UTF-8 bytes. `requestId` must be a UUID and `accepted` a Boolean.
Missing, duplicate or unknown fields are rejected. There is no candidate, device-target, name or legacy-decision
fallback. Native validation runs before global manager lookup.

## Receipt JSON

WASM methods resolve a string containing a `LiveboardOperationReceipt` object. Native methods return the same
object inside the existing success/message envelope:

```json
{
  "success": true,
  "message": {
    "kind": "recover",
    "secretId": "opaque-secret-id",
    "claimId": "opaque-claim-id",
    "phase": "queued"
  }
}
```

Receipt fields are `kind`, optional `secretId`, optional `claimId`, optional `requestId`, `phase` and optional
`errorCode`. Absent IDs are omitted. `kind` is `create`, `recover`, `join` or `removeDevice` for these operations.
Core supplies the actual persisted/queued identity, including a recreated Secret identity or an existing unique
Recovery Claim identity.

`phase` is `queued` or `unknown`. Neither phase nor native `success: true` means that the server completed the
operation, a peer approved it, membership changed, shares are locally ready, or a Secret may be revealed. An
`unknown` receipt preserves the known operation ID after an uncertain later failure. Clients refresh canonical Core
state and track that ID; they must not blindly submit a replacement Create/Recover or infer completion from the
receipt.

Checked Recover also verifies its exact returned Claim ID against the latest ordinary synchronization view: sender,
Vault, Recover kind and Pass must match. Missing, mismatched or unreadable canonical confirmation returns that same
ID with `phase: "unknown"` and fixed `errorCode: "RecoverCanonicalUnknown"`. Existing persistence/network Unknown
results retain their original code. This classification adds no request/packet delivery and never changes canonical
Accepted/Declined/Done, completes recovery or authorizes reveal. A healthy Queued receipt is still not a
cryptographic server acknowledgement or peer approval.

A previously processed raw-only Recovery record with no canonical claim is not automatically healed by these APIs.
Refresh/reopen preserves its actual local identity and reports Unknown; no history merge, replay exception,
replacement Claim ID or automatic resubmission is authorized. Clients surface the sanitized error and observe that
same ID through Core.

Before allocating a Recover claim, Core correlates unresolved local ownership using exact Claim/distribution IDs,
sender, Vault, kind, Pass and the original duplicate-free addressed receiver set. Receiver order is irrelevant;
mutable statuses, including requester Delivered outside that set, keep their existing lifecycle meaning. A unique
same-ID canonical identity conflict retains the actual local ID as `RecoverCanonicalUnknown` instead of allocating
a replacement. Multiple unresolved identities fail unavailable rather than picking the latest ID. Genuine
same-identity Declined/Done still permits the next explicit operation; an already active later claim is reused. The
correlation view never repairs or rewrites canonical data from local history.

## Checked recovered disclosure continuity

Successful native `show_recovered(claim_id)` keeps its existing String ABI and
`success/message.secret` envelope. It may additionally return
`message.revealReceipt` with `version:1`, exact `secretId`, `recoveryClaimId`,
`requesterId` and `revealContext`. Relevant Recover operations may carry the
same optional `revealContext`. Absence is not a new legacy Show failure and
grants no continuity fallback to a consumer requiring proof. Local Show and
legacy plaintext-returning Rust methods retain their existing contracts.

The v1 context contains `vaultName`, `currentDeviceId`, `vaultSource`,
`localDistributionSource`, sorted unique `recoverySources` and
`claimScopeCommitment`. Each source is an existing `ArtifactId` (`eventId`)
plus lowercase SHA256 `bodyCommitment`. Core hashes complete typed public or
ciphertext events including keys under separate
`MetaSecret.LiveboardReveal.{Vault,Distribution,Recovery}.v1` domains, followed
by newline and existing canonical JSON. It hashes no plaintext, decrypted
Key Share or derived Secret. The full unredacted Vault source binds membership
and revision; even an unrelated Vault revision can conservatively hide a card.

For the reveal-only Vault commitment, Core sorts only `value.secrets` and
`value.deletedSecrets` by the canonical UTF-8 bytes of each complete serialized
element before hashing the full event. It preserves every element, including
names and IDs, and every other Vault field and key; changed contents, membership
and revision remain significant. Generic canonical JSON, signed/stored events
and all other arrays retain their ordering. The domain and metadata version
remain v1. Old and rebuilt ephemeral proofs may disagree and fail closed;
there is no stored-proof migration, dual-hash acceptance or retagging of plaintext.

`ClaimScope` commits all complete canonical Split/Recover claims for the exact
Pass from all requesters, ordered by Claim ID. Only nonpersisted clientStatus
and the selected requester's expected absent→Delivered completion slot are
normalized. Accepted requires that slot absent; Done requires exactly Delivered
before normalization. Every other field, receiver order and status remains.
This typed projection commitment is not a fictional SsLog ArtifactId or a new
revision counter. New already-completed Claims and material/Vault changes
therefore cannot retain the previous fingerprint.

Core captures the receipt under the existing recovery owner from the SAME
actual Vault, local Distribution, complete Recovery packet set and decoded
reconstruction inputs, before Delivered/completion. It never rereads a later
tail to retag old plaintext. Accepted context reuses the SAME already-validated
Material inputs. Done exposes only an exact raw-source continuity candidate,
using current local readiness and no remote re-decryption. Only equality with
the original successful receipt proves that these are the formerly validated
remote ciphertext inputs; the candidate alone proves no new reveal eligibility.
Missing, conflicting, malformed, oversized or ambiguous sources omit metadata.

Existing signed completion/sync ordering is retained. Transport errors retain
their old failure behavior. Legacy transport can also return a decoded
`DataSyncResponse::Error` as Ok; its old success/value/error behavior is preserved,
but `revealReceipt` is omitted unless returned typed Data confirms the exact consumed Claim with requester Delivered and its approved receiver Sent. Empty, foreign, unknown or contradictory responses receive no receipt.
A successful legacy envelope alone therefore does not certify completion.

Metadata v1 accepts no unknown/duplicate fields, null proof, invalid sequence,
unsorted/duplicate sources or noncanonical commitment. Scalar identities are
limited to1024 UTF-8 bytes, context to8192 serialized bytes, receipt to12288,
and Recovery sources to1–2. Receipt/context Debug and errors are redacted.
These are output metadata limits, not new legacy reconstruction gates.

The current actionable Liveboard n3/k2 route requires the local share and one
approved remote share. Generic legacy recovery without a local share remains
supported where it previously succeeds, with receipt absent: current remote
packets do not preserve the original Split source. No-local Liveboard
continuity is not certified. A new Show on Done remains rejected by the existing
mobile policy. A receipt is never accepted as a command or replay credential.

Consumer integration is a separate Core-first gate. It must match the original
selected Accepted context, exact Show call and receipt, then current exact Done
context while preserving remaining Secret fields, full native owner/frame/read
readiness and privacy lifetime. It may reuse only the same already-owned card.
Hide/background/error/navigation/deletion/access change remain terminal; no
extra Show/auth/read, persistent plaintext or diagnostic authority is added.

Web compatibility intentionally drops optional Recover `revealContext` when projecting known fields;
existing known-field validation and permissions remain unchanged. It grants no new Web
retention or receipt authority. Native common JSON may contain `message.revealReceipt`;
the WASM `show_recovered` port still returns a plaintext String, not that JSON envelope.
[R32 focused compatibility](../.ai/artifacts/run/MS-20261019210543-005a-r32-web-compatibility.md) passed23 tests (5new+18existing) on unchanged production.
Fresh full Web/coverage/typecheck/lint/build, Task WASM packaging/generated ABI and real UI
E2E remain separate pending gates; historical665 results are not current R32 evidence.

## Delete responder provenance

A Delete result in the authoritative `VaultData.deletion_results` map retains its actual responder with
`resolutionSource: "canonical"`, including valid local requester decisions at one/two devices and actual peer
decisions at three. Later topology changes or tombstones do not erase that accepted history. An automatic
retirement available only in VaultLog preserves its exact IDs, outcome/status and settled readiness but has no
`responderId` and uses `resolutionSource: "unknown"`. It is not evidence of a signed device Decline; clients must
not animate an invented peer response. The authoritative exact request result wins a conflicting log-only marker.

The new projection/receipt excludes Secret values, ciphertext, Encrypted Key Shares, keys, signatures and internal
application/verification proofs. Secret plaintext is input only to Create and is not echoed into a receipt or
error.

## Actual Delete/Remove producer methods

These methods return a distinct version1 `LiveboardProducerReceipt`, not the older receipt shown above. Generic
managers enqueue one corresponding request on the same FIFO/private reply/executor and operation owner; native
wrappers use the existing singleton, FFI lock and runtime. WASM borrows supplied Pass/ Device IDs; native Delete
resolves the exact Core Pass identifier, not a name.

| Operation | WASM | Common JSON / UniFFI |
| --- | --- | --- |
| Delete | `delete_secret_checked(&MetaPasswordId)` | `delete_secret_checked(secret_id)` |
| Remove | `remove_member_checked(&DeviceId)` | `remove_member_checked(device_id)` |
| Observe only | `reconcile_producer_receipt(reference_json)` | same |
| Explicit same local removal | `continue_local_remove_checked(reference_json)` | same |

The receipt distinguishes actual request source, optional actual local decision source, local material stage and
canonical outcome/readiness. Sources contain the original ArtifactId and the shared full key+body commitment; this
digest is an identity, never authorization or proof of signature/admission. Actual Delete UUID comes from the
existing builder. Remove UUID is absent until an exact canonical occurrence is observed. Optional absent fields
stay omitted.

Source observations distinguish unconfirmed, locally persisted, admitted and raw acknowledged. Actual R/U attempt
identity survives write/readback failure and caller cancellation. `phase: queued` requires its applicable
acknowledged stages; it does not itself authorize reveal, mean an approval, or conflate canonical Applied with
local material readiness. New commands require capability `producerCorrelationVersion: 1`; an old/unsupported
endpoint cannot be presented as a successful producer. Legacy methods/envelopes remain unchanged.

`reconcile_producer_receipt` strictly parses/revalidates current actor, own-stream source body/hash, Vault/kind and
known UUID, restores any actual U omitted by the caller from durable Core context, and makes one atomic ServerTail
observation. It performs no client sync/get_app_state/finalization, material generation, append/signing, consent,
context save/clear or replacement operation. If both the input and matching durable context already name a
canonical UUID, a contradiction fails before the atomic query or any client write; it is not silently rebound. A
supplied exact U and supplied UUID remain in the observation reference after local context cleanup. A supplied U
starts Unconfirmed, never caller-asserted ACK; only the exact atomic server result can promote its status. A
nonexistent U or contradictory UUID cannot silently inherit the completed R acknowledgement. It cannot make
transport progress from an unsupported/redacted original source.

Local2→1 keeps one compact active R/U/material context in a separate optional field of the existing local-only
removal registry. It retains actual source identities before body attempts and never reinterprets guarded selector
entries or their managed boundary. An active still-free source reservation refuses a foreign legacy body. Explicit
continuation of an unfinished U still requires the captured exact two-member Vault and existing local approval
role; absent attempted U may only be rebuilt identically at its SAME owned free key. No R2/U2, new signature for
changed historical ciphertext, stored consent or automatic restart/resubmission is created.

After this exact R/U has already Applied and ordinary synchronization downloaded the singleton Vault, the same
explicit continuation has a separate completion- only path. It requires the exact current surviving actor/target,
actual existing R/U bodies, one mandatory fresh successful atomic query acknowledging BOTH sources and exact
Applied UUID, a unique matching accepted local canonical result, and actual local material/readiness. A stale
Pending/Unknown context marker does not replace material proof or permanently prevent cleanup. Only verified
same-context observation/material bookkeeping and exact clear/readback are allowed. Query failure cannot clear from
older acknowledged facts; this path cannot sync/upload, append/sign, rebuild U or regenerate material. The pre-U
changed-two-member guard remains unchanged, and observation-only reconcile never clears the context.

Current migration limits remain explicit: the separate technical Noop protocol is owner-approved and server layers
A/B are implemented. Checked guarded C1b private preparation/queueing has independent source acceptance. Initial C2
freezes the original compact signed source and publishes frozen ordinary material only after exact local
Applied/current-context proof. Selected C2/C3 publication, transport/privacy/receipt paths have bounded evidence.
Healthy explicit rollover now reaches actual full sync and Member3 GetState, explicit Accept at N+1, true same-file
SQLite reopen and real admission/separate surviving K1 recovery. Exact compact N remains immutable; its technical
receipt is not N+1 Business approval. Fresh and reopened FULL retry verify retained proof with the current user
before append and reuse frozen crypto/IDs. This adds no binding field or public command. Supplied CURRENT technical
proof must bind its own origin before explicit append; valid partial CURRENT proof is not settled HISTORY or
Business approval. Required old receipt+confirmed-tail checks remain unchanged. Selected other-request winner
retirement has bounded host evidence through real Noop settlement/separate Tail and the first SS boundary. It
requires the intact losing Pending/FULL/origin with no same-UUID result, exact surviving context and the different
Accepted/Applied UUID's exact roles/source. Active inputs and exact own Applied publication remain protected. This
adds no binding field, business result or client-side approval rule; native package/UI acceptance remains separate.
Bounded Core-service Reset returns Local only after the same persisted Declined session/removal proof and final
identity/status revalidation through cleanup. Unproven sync/finalizer failures retain the original error. Existing
client/sync owners coordinate GetState and local recovery writers; shared helpers verify full current credentials
before local completion. Host evidence includes real cleanup and no late writer resurrection, with no new
binding/export or approval rule. Mobile's failed pre-sync uses a Rust-only service handoff to the same terminal
proof, without FIFO/network work; unproven or failed proof preserves the original error. Its healthy two-pass/FIFO
path remains unchanged. Actual-manager host tests cover this caller; no JSON/FFI export or native package/UI
acceptance is added. The first incoming C4 host case now returns two actual same-object Member3 states after
successful FIRST addressed current-K2 import and repeat, retaining deleted-p0 cleanup, the same Pending UUID and
exact technical compact. This adds no binding field, reveal permission or approval result. Member2 owned
finalization, three outgoing selectors, later Join2→3 and native/UI acceptance remain separate. Final rollover
source review, later fault/negative groups, selectors/member2 owned finalizer and complete C3–C5/client integration
remain separate pending gates. This does not enable retirement of the separate local R/U producer context. [Private
persistence/receipt boundaries](guarded-preparation.md) add no binding fields or new command authority. Host tests
are not generated native/WASM packages, client integration or UI E2E acceptance. The whole-copy compatibility rule
below is separate from signed command authority and server source admission.

## Whole-copy material availability

One non-revealing Core loader reads the exact typed own Distribution (Pass, recipient), verifies its
descriptor/key, data Pass/Vault and intended recipient, and decodes the Encrypted Key Share DTO. Projection and
producer readiness retain their exact canonical Split map-key/id/distribution-id/Vault/type binding. Public
Web/common Show keep the existing current Core `ensure_local_secret_available` guard and reuse the same
loader/structure check; they do NOT gain a canonical SsLog/history dependency. Only that explicitly authorized Show
action subsequently calls reconstruction. Availability never calls a plaintext Show/combine/recover API, writes
data, synchronizes or creates a new generation.

For current one/two members, Core's existing local-copy policy permits a coherent stored `(n=1,k=1)` or `(n=2,k=1)`
copy. Stored N describes its generation, not the current member count. After2→1 the administrative Split owner
changes to the survivor while the original encrypted k1 copy, generation and ciphertext origin remain unchanged. An
old ciphertext origin is not a current-role grant and need not match the mutable current owner. No new
historical-origin anchor/signature is required for valid existing data. Exact current actor, live Pass, own
recipient and material identity still constrain access.

Every block retains its own valid metadata length (for example a65-byte Secret has64+1), while stored n/k is
coherent across blocks and share_id is within the stored N. Encoded blocks remain113bytes with existing
nonzero/metadata bounds. At three members and for recovery material, validation remains exact current n3/k2 with
the existing source/member/approved-receiver and compatible-generation checks; old k1 data cannot supply ShowLocal
or bypass consent/quorum. No Shamir, resharing, signature, ciphertext, claim generation or approval rule changes.

## Web recovered Show delivery

This section describes the implemented Web client ownership around the existing Core Show port, not a new
WASM/UniFFI API or Core eligibility rule. A successful raw `show_recovered` return alone is not a persisted
acknowledgement. Exact fresh Core Done and the actual one-call returned value are both required for first delivery;
neither substitutes for the other. Passive Done never invokes Show.

1. A deliberate visible Show creates one nonserializable local owner for its exact
   Pass/Claim/card incarnation and current presentation. The adapter holds the exact
   owned Claim through one borrowed SDK await, frees it once in finally and never
   resubmits on cancellation or failure. Missing coordination in isolated direct
   mode retains its strict epoch fence; failed coordination has no fallback.
2. Raw returned text stays in one private unfinished capsule. No decode/seed copy,
   DOM/ref/Pinia/storage/URL/log/canvas exposure precedes fresh proof. Hard revoke
   synchronously drops references; a late SDK return is discarded. Physical JS
   string zeroization or GC timing is not claimed.
3. Before retiring the old SSE generation, the same parent synchronously raises
   its existing required barrier and clears ALL already-held plaintext/Copy/motion.
   `restartReady` opens the same authenticated stream owner. Successful HTTP
   response with body follows server authentication/subscription and supplies
   readiness; there is no initial ready frame or replay. Retired callbacks are
   generation/abort fenced; scope, timing or an unused revision is not an ACK.
4. A separate module-level metadata-only gap branch owns the post-header read,
   independently of the pending Show owner. Even when filtering cancels only A,
   B is already concealed and the parent still advances a NEW required target
   after HTTP readiness. Its existing coordinator/Store lane queues a physical
   sync/read behind pre-ready flights. Those earlier/coalesced reads cannot satisfy
   this target. The branch contains no Pending, card, raw capsule or delivery
   callback; closing the generic gap grants no Secret permission.
5. The parent identity/key/manager/generation/revision, active eligibility, current
   lease and target are rechecked across awaits and reactive blocking. Final proof
   additionally requires a ready member context with no failed/pending sync, the
   exact fresh accepted Done for the same Claim/Pass/requester, SAME Secret's
   Core-ready Recover instruction and unchanged relevant canonical history.
   New/missing/changed claims, deletion/recreation, actor/provenance/member changes
   or Done regression permanently revoke. No count/name/opaque-ID ordering supplies
   permission or continuity; Core remains the domain/security owner.
6. Only a still-live original owner may decode and deliver synchronously once
   after the new current render fence, without another await or SDK action. The
   pure delivery boundary matches its actual private owner by object identity
   before narrowing unknown input. True delivery alone consumes exact unread;
   its finished-owner marker carries no second plaintext payload. Capsule and
   temporary lease listeners are removed, with no finished-value resume cache.

The persistent parent stream-loss callback survives that temporary cleanup. Current EOF, transport/auth failure or
missing body conceals held UI and latches the required barrier even after first publication. New-stream
invalidation is always hard revocation. Route/filter/Hide/modal may cancel delivery but cannot abandon an eligible
parent's gap read. Hidden/offline work defers behind its latched barrier until ordinary eligible pageshow/online
required refresh. Full auth/key/manager/member/Reset/disposal changes fence old work from a successor. Failures
remain blocked across cached reads and error dismissal. No callback, reconnect/readiness or finally restores a
value or repeats Show/Recover/auth. Plaintext is not persisted or logged; raw values do not enter operation
metadata.

For run MS-20261019210543, the accepted Recovery source handoff has222 passing composed client/controller/Store/SSE
checks. This is the source contract plus component evidence; actual real-Core Stage9 Recover continuation, final
original UI E2E and native UI migration remain separate pending gates. See the [client implementation
boundary](../meta-secret/web-cli/ui/src/features/liveboard/README.md#first-recovered-show-delivery).

## Web manual04 errors and required reads

Every explicit controller report is a synchronous privacy event, even with the same sanitized04 message. A
structural setter scope avoids only duplicate watcher handling; internal notification error writes retain that
fallback. Current controller/revision binding revokes unfinished recovered capsules and latches the existing parent
barrier, clearing all held values, Copy and motion. sessionError owns metadata/callbacks only, with no second
manager, lane, stream, poller or independent readiness policy.

Within the same parent,04 errors remain until manual tap. That tap clears display, rechecks the current owner after
reactive effects and asks the existing coordinator for required physical sync_now→get_state. Only its current
successful proof can restore readiness; dismissal, cache, older in-flight reads and time cannot. Failure stays
blocked with a sanitized manual error. Ordinary genuine sync may restore technical readiness while the earlier04
error stays pinned. Hidden/offline work waits for normal eligible resume;
auth/key/manager/generation/revision/member/Reset/disposal fences keep stale work from reading or certifying a
successor. No cleared value/Copy/motion or unfinished capsule is restored; Show remains explicit. Typed FIFO/exact
decisions, pending Unknown and original R/U survive; no automatic auth/Show/Copy/business retry or Core
permission/receipt/protocol change is added.

Authored Create input stays in ONE transient form owner, including inactive Plain value, mode/count and all24 Seed
words. Ordinary SAME-session sync/SSE/ready=false preserves OPEN/CLOSED and full input while blocking writes and
old clipboard results. An explicit, internal or equal error closes(clear:false), pins04 and preserves SAME input
for deliberate reopen after readiness; freshness never reopens it automatically. Route conceal also
closes/preserves. Actual background/key/member/identity/Reset/disposal and idle Cancel hard-clear input. A hard
view clear does not release the original pending/Unknown write; the existing genuine owner-change rules remain
distinct. No Web Create OS-auth is added.

Parent admission is privately fenced before observable error/sync/projection callbacks. Each new write owns a new
ticket and clears old receipt metadata before calling its port; reconciliation captures and rechecks the exact
original receipt/ticket/operationId. Private terminal consumption precedes observable callbacks; an oldA cannot
release newerB across hard conceal. Exact canonical success clears even a closed original form, without opening it
or submitting again. Soft edit/paste invalidation never discards admitted result lifetime. Actual App checked
Create has submitted/Unknown/Uncertain outcomes; knownRejected coverage is a separately labelled typed parent-port
contract, not an invented WASM outcome.

[Current bounded validation](../.ai/artifacts/run/MS-20261019210543-005b-web-create-parent-green.md) is58
selected/665 full unit/component tests plus typecheck/lint/client build, with unchanged WASM packaging.
Standard63-module coverage is95.8% lines,89.69% branches,88.4% functions; 80% stays advisory and gaps remain
recorded. The [independent review](../.ai/artifacts/run/MS-20261019210543-005c-web-create-parent-final-review.md)
preserves original RED/loader/partial/tooling histories, exactly two reviewed obsolete draft-wipe oracle amendments
and the distinction between direct probes and source-only postcallback protection. Lint has0 errors plus8146
existing warnings outside changed files. These tests mock Core/OS/network and use reduced-motion DOM controls; real
browser/Core UI E2E, normal-motion design fidelity and native renderer/OS acceptance are not implied.

## Boundary errors

`ProducerReference::parse_json` is the separate version1 grammar for new producer references; it does not change
existing checked receipt JSON. Input is limited to 4096 UTF-8 bytes. Every object, including `eventId.fqdn` and
`eventId.id`, rejects unknown and duplicate fields. Full integer `curr/prev` is retained in the existing ArtifactId
type; `curr` must be positive and `prev == curr - 1`. Parsing does not round through a floating-point or JavaScript
Number representation, including for valid source sequences above2^53. Bindings must preserve Core-supplied
integers.

Kind is exactly `deleteSecret` or `removeDevice`, commitment is 64 lowercase hex characters and a supplied request
UUID uses its canonical lowercase hyphenated form. Delete requires its actual request UUID and omits
`decisionStage`. Remove may omit an unknown canonical UUID and may carry an actually attempted local decision
source. Required sources cannot be null; caller observations, readiness, material stages and confirmation flags are
not accepted as reference input.

Malformed input returns only `InvalidProducerReference`, without echoing JSON or Serde diagnostics. The
ProducerReference Deserialize implementation calls the same strict grammar, including the actual ServerTailRequest
wire field; it is not a parser-entry-only guarantee. See the [server correlation
contract](liveboard-server-correlation.md) for the atomic query, verified source identity and public/private
projection. Successful parsing proves grammar only: actual source owner, Vault, commitment, admission, current
authority and canonical status need their selected Core validation. This parser does not append, synchronize,
reconcile or authorize any operation. Producer/query/continuation behavior and package adoption remain separately
gated.

Native failure has no message or raw exception:

```json
{"success":false,"errorCode":"InvalidDecisionInput"}
```

`InvalidDecisionInput` is returned by the exact-decision parser; `NotInitialized` means there is no global native
manager; `OperationUnavailable` covers execution/serialization/lock errors without returning caller input or
underlying diagnostics. WASM rejects with a JavaScript Error containing the fixed `InvalidDecisionInput` or
`OperationUnavailable` code. A valid request still requires normal Core/server authorization.

Legacy exports, legacy response envelopes, approval rules, cryptography and K-of-N behavior are unchanged. Host
binding tests are not evidence that a generated browser, Android or iOS package has been shipped or exercised.
