# Initial Recovery request durability

Ordinarily verified, Core-shaped initial Recover requests use one temporary
metadata-only intake receipt in their canonical SsLog record. This prevents raw sequence
advancement without a proved canonical claim. It does not heal historical
raw-only orphans, authorize a replay, create a new request or change peer policy.

## Admission and exact identity

The normal SignedAction/Event verifier runs first under
`ServerApp::handle_client_request`'s existing request owner. The intake helper
separates immutable initial source shape from actual admission context. Shape
means Recover, matching claim/dist-claim ID, the requester's exact SsDeviceLog
descriptor, positive original sequence, duplicate-free nonself original receivers
and exactly their Pending statuses with no computed clientStatus. Receiver order
and the empty original set are retained; no historical membership proof or
current-receiver count/equality is inferred. Admission still requires the
original requester to be a current member of the source Vault with an available
exact Pass.
The canonical claim must be absent and no other active same-owner/Pass Recover
may exist. IDs, nonce, body, receiver order and signed bytes are preserved.

An initial source created before an ordinary removal or join can have different
receivers from today's members. It uses the same durable intake while retaining
its exact original source and signed bytes. The receipt captures the actual
Vault ArtifactId at admission, after the earlier topology change. Reopen shares
the same shape/context checks and still requires exact equality with that
captured admission Vault. A later context change cannot reanchor Prepared.

Eligibility remains narrower than legacy snapshot acceptance. Same-ID/terminal
snapshots retain the existing monotonic canonical merge; Split, a different
already-active claim, malformed legacy snapshots and tombstoned sequence-only
sources gain no new continuation authority. Their existing routing is preserved,
including authenticated malformed input the legacy processor actually accepts;
this is not new general rejection policy. B2 does not claim initial-intake
durability for those distinct branches. First peer response and requester Sent
completion remain separate roles; a removed original receiver gains no response
right merely because its ID remains in the original receiver vector.

The previous current-topology equality left a legitimate A-signed `{B,C}` source
unprotected after ordinary removal of C, and an earlier subset after an ordinary
join. Independent 21-case TDD reproduced those gaps through actual signed
topology commands and real transfer material. All 21 now pass with the shared
predicate fix, including late continuation and real SQLite/ReDB close/reopen
assertions. Independent implementation review remains pending; prior 005b/005c
and the independent Fail stay historical evidence. Executed GREEN is not a
complete security, client or release gate.

## One fixed record, then original raw, then atomic cleanup

The canonical save contains the exact original initial claim and optional
`pendingRecoveryRequestIntake`. The receipt holds version, Claim/requester IDs,
original KvKey, canonical source/action hashes, verified Vault ArtifactId and
fixed canonical base/target IDs. It has no body copy, signature, SignedAction,
ciphertext, Key Share, key, Secret, responder or outcome. Hashes are correlation
metadata, not authorization tokens. Clients cannot submit canonical SsLog state.

1. Save the intended canonical target and require returned key/full readback.
2. Save/read back the exact original raw source at its original key. An existing
   equal raw value is continuation; different data is unavailable.
3. Atomically replace the same canonical target, clearing only the optional
   receipt, then require key/full equality readback.
4. Publish ordinary SsClaims invalidation and acknowledge only after cleanup.

SQLite already provides a single UPDATE, ReDB a single-record transaction and
InMem a locked replacement. No new table or private-store capability is added;
unsupported replacement remains an error. This is not a cross-record transaction
or distributed-writer guarantee. There is one actual ServerApp request owner.

A save/drop/wrong-key/readback/lost-reply failure returns unavailable/Unknown.
If Prepared actually persisted, it survives reopen. If cleanup actually committed
but its reply was lost, no receipt remains and no raw is rewritten. Ordinary
Event replay still rejects; the next normal ServerTail/sync observes the same
Claim ID. No exact Event acknowledgement exception is introduced.

## Whole-scope audit before repair or effects

Before the requested read/write, read-only discovery covers explicit Vaults,
actual owners of all three arbitrary Generic Vault tails, SsRequest history and
its sender-Vault fallback. Each affected canonical SsLog stream is scanned from
first ID to actual head, plus separately addressed records beyond a stream gap.
Identical physical records are deduplicated with equality; two physical Prepared
records are not one receipt merely because their fields match.

Zero receipts is clean. Exactly one is eligible only at its actual current head
and fixed target, with complete source/hash/base/target reconstruction, captured
Vault context and no conflicting raw/private-response/Removal preparation.
An addressed Prepared owned by a different request Vault is unavailable; this
does not change clean legacy read policy. Old, multiple, copied/wrong-target,
malformed, out-of-stream or conflicting receipts return the static
`RecoveryRequestIntake.Unavailable` before any affected Vault is repaired, status
is written, packet delivered/deleted, membership changed or signal published.

Only after every candidate passes that read-only audit may the owner finish the
unique current intake. It reconstructs the exact source from the same fixed
snapshot, compares the saved body hash and captured Vault, finishes raw and cleans
that record. Then existing private receiver-response repair, Removal repair and
ordinary operation proceed. A non-head receipt is never repaired into a new head.

Low-level public Rust Vault/SsRequest response builders perform their own
read-only zero-receipt preflight before status/delivery effects. They do not own
repair or recursively call ServerApp. Per-record/final-vector checks reject any
Prepared SsLog before serialization; completion checks source and generated logs
before persistence/publication. This protects the intermediate field without a
new caller-tail allowlist or broad read-auth redesign. Unowned concurrent direct
gateway use is not advertised as serialized operation.

These scans cost O(addressed records + affected canonical histories), and the
low-level builder repeats clean preflight. No cache/index or age-based shortcut
is used; measure before proposing an optimization that might miss older receipts.

## Compatibility and limits

Old SsLog JSON defaults the field to None; existing signed DTOs, public command
and response variants, actor/replay/quorum/crypto and completion/reveal rules stay
unchanged. A successful response contains no pending field because atomic cleanup
replaces that very target before reads. The real HTTP queued, embedded and SSE
authorization paths use ServerApp; live SSE emits opaque signals only.

Historical raw-only orphan records lack trusted original context/receipt. They
remain exact-ID Unknown under checked Recover; no history scan, new signature,
ABA guess or replacement claim revives them. The original regression now seeds
that documented pre-upgrade state, retaining real source/signature identity,
reopened tail, Unknown/no-healing/no-duplicate assertions and old RED/B1 evidence.

Do not run mixed/downgraded server binaries while a new receipt is Prepared;
old binaries cannot enforce a field they do not understand. No deployment or
client artifact regeneration is performed for this internal field alone.

Run20261019210543 has 48 current-context and 21 in-flight focused backend tests,
including real SQLite/ReDB reopen, and the original compatibility seed. All
69 focused cases pass; actual removal/join, original signed source, later
continuation and no-rewrite assertions are executed, not just authored. These
are backend tests, not UI E2E,
secret reconstruction proof or final security approval. Independent source/
security review, remaining explicitly identified plan cases and ordinary owning
workflow/client gates remain necessary before a release/handoff claim.
