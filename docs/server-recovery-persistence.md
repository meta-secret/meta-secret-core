# Server-private Recovery response persistence

The first authenticated Secret Recovery Approve/Decline is durably reserved
before its normal raw workflow packet is written. This is one temporary record
per Vault and one exact Claim/source. It is not a command journal, an approval
policy, a plaintext store, a replacement for cryptography, or reveal readiness.

## Storage and startup

`KvLogEventRepo::server_recovery_response_store` exposes only readiness, load,
reserve, source-conditional mark-committed and conditional committed deletion.
ServerApp initialization and the actual queued run loop require this capability
before accepting work. Unsupported client-only repositories cannot be used as a
server backend; there is no fresh/volatile fallback.

- Production HTTP uses SQLite. An additive `server_recovery_response_private`
  table in the same DB file has a Vault primary key and private binary record.
  Readiness initializes only this table idempotently; it does not clear rows.
  Compare/reserve/phase/delete each use an SQLite immediate transaction and the
  existing busy timeout.
- ReDB uses the same database handle and a separate
  `server-recovery-response-private-v1` table. Each compare/change is in one
  write transaction; actual file reopen retains the row.
- InMem owns a separate private map for its repository lifetime. Its public
  event map/get_db never contains the record. A recreated ServerApp on the same
  repository therefore repairs the same reservation.

No ObjectDescriptor, ArtifactId, GenericKvLogEvent or DbLogEvent conversion
exists for this store. Generic network tails cannot select either private table.
Whole owned database reset clears both stores; normal server startup never does.
The guarantee is bounded repair under one server request owner, not distributed
writers or a cross-record transaction.

## Phases and exact identity

Prepared stores the original already-verified signed encrypted packet/decline,
its full-envelope SHA-256 source proof, Claim ID and fixed base/target SsLog IDs.
Reserve is insert-if-absent; occupied rows must match the complete prepared
record/packet. Raw save/readback uses that same original workflow ID. The
existing status transition and compact winner/source proof commit together in
one fixed canonical SsLog snapshot; conflicting raw/target content is unavailable.
There is no fresh share generation or new winner on retry.

Committed atomically removes the retained packet from the private record. A
Prepared row whose canonical fixed target already contains the same committed
source takes this branch before any raw save. This prevents reconstruction of a
packet already delivered and consumed by the requester after an earlier private
phase-write failure. Committed repair verifies the canonical proof and only
finishes conditional private cleanup/readback. Cleanup errors remain errors.

A new compact optional `RecoveryDecisionRecord.source` contains only workflow
ID, complete signed-envelope hash, actual verified responder and outcome. It
never contains the packet. After private deletion, only an authenticated exact
source match may receive a no-mutation acknowledgement. A changed source/body,
nonce, actor, claim, channel, Vault or outcome cannot use this exception.
Historical proofless replays remain rejected; no winner is inferred from old raw
history. Existing signed action/claim/completion shapes do not change.

## Request ordering and privacy

Under ServerApp's existing request owner, side-effect-free discovery follows
explicit request Vaults, all actual generic Vault read-tail streams, SsRequest's
actual tail and its existing latest fallback. For each sorted/deduplicated Vault:

1. Repair the private Recovery reservation and verify cleanup.
2. Repair any existing RemovalApplication.
3. Initialize historical pending removal IDs where applicable.
4. Execute the normal requested action, retirement/purge or read.

Discovery reads repository events without invoking recipient delivery or
consuming raw packets. A forged different sender Vault cannot skip repair of the
actual Vault addressed by a supplied tail. This does not add a broad read-auth
redesign; the original operation still passes its normal verifier.

Private records have redacted Debug, no argument-capturing instrumentation, and
static UnsupportedBackend/Unavailable/CorruptRecord/Conflict errors. Underlying
DB/decoder causes are dropped rather than retained in an error chain. The
private row/packet never enters public SsLog history, invalidation events,
Liveboard projection, FFI or network response construction. Existing normal
recipient-specific raw ciphertext delivery/deletion is preserved.

Raw Approve/Decline still requires a canonical addressed receiver and cannot be
answered by the requester. The requester's valid signed Sent completion after
approval retains its distinct existing role. Crypto, k-of-n, first-response,
receiver selection, membership/removal quorum and actual reveal are unchanged.

## Evidence and release limits

Run20261019210543's independent113-case receipt suite covers actual SQLite/ReDB
reopen, both outcomes and before/lost-response/readback faults, no resurrection,
changed-source refusal, corruption, TRACE privacy, generic-tail discovery,
startup and cleanup-before-removal/tombstone ordering. The expanded server target
passes161 tests, including the original raw-before-SsLog failure regression and
requester authorization controls. Details are in the run's005b/005c receipt reports.
These Rust backend checks are not HTTP, browser/native or Core UI E2E results.
Independent source/security review and the remaining client/guarded preparation
work still precede adoption/release.
