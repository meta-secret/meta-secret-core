# Device transport Noop: server boundary and client integration status

The owner-approved transport allowance is implemented on the server (layers A/B).
Client C1b implements fixed private all-Pass preparation and exact origin-before-body
queueing with independent source acceptance. Selected C2 publication and C3
retirement/privacy/receipt paths have bounded host evidence. Healthy explicit
rollover has real SQLite reopen evidence; final source review and later faults
remain separate. The complete client C3–C5 lifetime is still pending.
This document does not establish full Core handoff, UI E2E, native/WASM package
readiness or deployment approval.

## Authority, identity and durable effect

A distinct `WriteSyncRequest::DeviceTransportNoop` carries version1, purpose
`meta-secret.device-transport-noop.v1`, Vault/actor, exact event ID/stream/nonce,
an opaque lowercase SHA256 client commitment, and its own DSA signature. Canonical
JSON of the eight unsigned flat fields is signed; canonical JSON of the complete
signed flat authorization supplies `authorizationHash`. No original Business
body, encrypted evidence, UUID revision or historical queue proof is required
or inferred by the server. Nested new-wire fields are strict; old signed schemas
are unchanged.

Under the existing ServerApp request owner, only a CURRENT registered canonical
Member may occupy its OWN exact next EMPTY contiguous DeviceLog position. The
dedicated verifier has no caller actor/bootstrap-key fallback. A sameN ordinary
Business or a different marker conflicts; neither is replaced. The marker never
dispatches a domain action, answers a request, changes quorum, authorizes reveal,
or changes a canonical result. This intentionally grants a current member a
broader own-stream technical permission than the pending honest client selector.

Insert plus exact equality readback linearizes the marker/receipt. Lost save
reply can succeed when actual persisted equality proves it; drop/uncertain
readback cannot advertise commit. Same signed retry after reopen returns the
same stable receipt even after real ordinary N+1. Receipt identity has no mutable
tail. Persistence errors at this new boundary become static
`DeviceTransportNoopUnavailable`, including the actual queued server error path.

`recordEffect:not_applied` describes ONLY the technical record. Existing exact
canonical Business outcome/source proof remains authoritative across an earlier
canonical Applied/raw-save gap. Producer observation validates the technical
slot and retains canonical Pending/Applied with source observation `Admitted`;
it never reports the marker as original Business `RawAcknowledged`.

## Typed readers, cleanup and public delivery

The dedicated `DeviceTransportSlot` scanner validates every actual key, complete
sequence, descriptor, owner/Vault and marker signature/key binding. Only explicit
Business and Noop variants are allowed. No catch-cast-error/skip generic traversal
is introduced. Tail/audit readers count the technical position; cleanup projects
Business payloads and continues past markers to later real ciphertext.

Mandatory Secret tombstone purge covers both full DeviceLog scans and exact
indexed admitted source refs beyond a prefix gap. A validated Noop has no transfer
payload; its bytes/signature remain unchanged. Later retained Business transfers
are scrubbed with exact replacement readback; original compact source commitment
is not recalculated from redacted bytes. The same collector protects direct reads.

Public Vault/status/Ss tail namespaces are validated BEFORE discovery/repair.
Allowed canonical records are typed and checked for physical key/descriptor and
payload context. SsLog whole history, latest fallback and beyond-head caller
paths share those checks before discovery/audit/maintenance. No marker/private
record/signature is serialized by canonical public reads.

Ss delivery preflights the COMPLETE addressed workflow set before the first
packet deletion, Split completion, canonical SsLog save or invalidation. Each
actual key/full sequence, expected subtype, Vault, Pass and addressed ciphertext
recipient is checked. An invalid later item cannot consume an earlier valid
packet. This preflight adds no transactional success claim for later independent
storage failures. Healthy delivery retains the existing delete/completion order.
Retained same-Pass claims are deduplicated by physical workflow ID while retaining
the first visited claim association; delivery/delete occurs once. This selects
no newest-generation or claim-ID winner.

Split preserves existing current-value `(Pass,receiver)` semantics even if old
claim receivers are stale. It adds no equality to historical generation, current
member count or mutable canonical sender. Recovery delivery reuses the existing
signed-intake claim/responder/channel binding; requester completion stays separate
and first-response/source metadata is unchanged. No ciphertext is decrypted here.

## Compatibility and pending C

The unsigned capability `deviceTransportNoopVersion` defaults to0 when absent;
the new server advertises1. Existing legitimate signed Business and canonical
client wire shapes remain. Arbitrary/private/cross-context read tails were never
an authorized marker export route. Generic signed Event cannot mint the new kind.
SQLite/ReDB marker codecs and real close/reopen retries are covered by selected
component tests. An old server binary cannot read a new-marker database; binary
downgrade requires an explicit compatible backup/data migration. No deployment
or data migration is performed by this work.

C1b requires capability1 before guarded preparation/queue, freezes all-Pass outputs
in one fixed private record and reads back exact owned origin before body.
[Private preparation persistence](guarded-preparation.md) documents the specific
index/revision barrier, retry identity and prebody-versus-postbody receipt truth.
C2 initial publication now requires exact locally imported own Applied source and
current context, preserves frozen packets/actual claim IDs and scrubs duplicates
after proven ordinary handoff. Its postimport/end work holds the shared owner.
C3 now includes selected Vault-only preflush/posterror retirement, mandatory
private/ordinary purge and per-ID authorization/receipt/separate-tail component
checks. Healthy explicit rollover preserves exact compact N while a new explicit
decision installs a complete new stage at the same witness key. Both persisted
settlement proofs are required; the actual user's key verifies retained technical
history before fresh or reopened FULL append. N's NotApplied receipt never
acknowledges N+1 Business, permits reveal or triggers a new decision. Index and
witness writes are existing-only with exact readbacks; no protocol rule changes.
Optional CURRENT technical metadata is verified against THAT current origin and
actual user before explicit append; it cannot borrow N's proof for N+1. Valid
authorization-only or receipt-without-confirmed-tail CURRENT stays partial;
supplied tail still requires its receipt. Required settled
HISTORY remains strict. No authorization or receipt is manufactured.
The selected other-request winner path additionally requires intact local
Pending/FULL/origin, no current-UUID result of ANY phase, exact surviving
owner/two members and target Declined, plus the different Accepted/Applied
UUID's reversed roles and exact responder source. Existing actual body/Guarded
authorization and auth-readback-before-scrub remain mandatory. It settles only
the abandoned transport N, preserves active inputs and own Applied promotion,
and leaves native package/UI acceptance and remaining C4 ownership gates separate.
The first incoming C4 host case additionally reaches full SS import of actual
current K2 at a retired prospective K1's reused key, followed by repeat and two
Member3 GetState calls. Exact compact/auth/receipt/separate-tail and redacted N
remain unchanged; Noop grants neither packet ownership nor readiness. Exact old
generation exclusion and applicable published-own local proof remain mandatory.
The separate service Reset path now reaches actual Local/cleanup after a real
removed-actor Unauthorized. It requires exact persisted Declined/session/removal
proof, revalidated under client→sync through cleanup; unproven cases retain the
original error. Existing service/shared recovery writers cannot cross that
automatic cleanup interval, and stale shared-helper credentials are rejected.
This neither settles the old Noop slot nor weakens server membership checks.
Mobile's failed pre-sync now invokes the same terminal proof through one Rust-only
service handoff, with no extra network/FIFO pass; only proven cleanup yields Local.
Other outcomes retain the original error. Actual-manager host tests preserve the
healthy two-pass Recovery upload/FIFO path. Native packages/UI and global/manual
Reset remain separate.
Later rollover faults/negatives, all selectors, owned finalizer and later2→3
lifetime remain separate C3–C5 gates. Same-UUID logical Pending remains Pending
after transport retirement. Old missing origin is not manufactured from topology;
local Remove-producer R/U and Legacy retirement are outside the selected C scope.
The historical private-preparation16 is not claimed byte-identical to the current
phase-adapted component suite; exact source extents/results are recorded by the
workflow artifacts. These results are not client package or UI E2E acceptance.
