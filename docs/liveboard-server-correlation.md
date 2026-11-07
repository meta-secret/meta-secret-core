# Exact Liveboard producer correlation on the server

Current server implementation status: post-raw cleanup, current canonical Member/UserData context and direct canonical-log cleanliness pass all12 guard regressions and full server299, with strict production/test Clippy. Independent final source recheck remains required. Prior Core274 checks remain historical. No client/package, Noop or full UI migration readiness is implied.

This describes the version1 server metadata contract. The checked client producers and generated package adoption have their own integration gates. Existing legacy checked Create/Recover/decision receipt JSON and signed domain action shapes are unchanged.

## Identity and observation

A new eligible signed Delete or Remove source gets a server-owned exact association: original complete DeviceLog ArtifactId, Core body commitment, actual actor/Vault/subject and actual canonical request UUID. Delete carries its real UUID; Remove does not invent one before server admission. Local2→1's actually attempted acceptance source can be associated with the same Remove UUID. No names, target-only matching, first/latest history or caller-provided hashes establish admission.

The shared Rust helper `node::common::model::liveboard_producer::producer_source_commitment` hashes exactly `MetaSecret.LiveboardProducer.Source.v1`, one LF and existing canonical JSON of the entire original DeviceLogObject including complete key. It returns a lowercase SHA-256 string. The helper/hash/reference is not signature verification, current authority or proof of key possession. Internal verified SignedAction digest is never included in an observation.

Optional `ServerTailRequest.producerReference` is observed with the existing NEXT raw upload position in the same ordered server owner. NEXT equal to attempted N is a free position; only a later NEXT has passed N, and raw-tail movement alone is not canonical admission or operation success. Missing/legacy capability or missing proven source remains Unknown. Optional canonical Remove UUID remains absent until actual proof. This unsigned query does not authenticate ownership of the named sender's private key.

A source can be admitted before raw acknowledgement. Canonical Pending, prepared application, Applied and Declined are separate from raw receipt and material readiness. Read-only client reconciliation does not synchronize, append, re-sign, decide, request biometrics, allocate another command or infer cancellation from absence. Server maintenance may finish an already admitted prepared effect before publishing a consistent observation; it grants no new caller authority.

## Automatic Delete completion

At actual eligible one/two-device admission, the server saves pending P, verified source reference and one bounded Prepared application in a single verified VaultLog snapshot. The application stores actual P/source and frozen base/target Vault ArtifactIds. Repair reapplies only the same existing authorized automatic effect and requires exact target readback. It then persists terminal P plus PurgePending, removes the deleted encrypted workflows and retained transfers, reloads the latest scrubbed log and persists AutomaticComplete. Only completed cleanup allows the SAME still-fresh raw source to be acknowledged.

Before-write faults, silent drops, post-write lost replies, conflicts, readback failure and backend reopen retain exact identities and phase. An existing target/result is reconciled without new P/UUID/signature/share generation. Invalid raw intake gains no acknowledgement from maintenance. Ordinary old Event replay is still rejected. A three-device request keeps its actual remote Pending mode after topology changes; current count does not manufacture automatic consent.

Retained removal packets are scrubbed both through existing contiguous raw history and exact already verified source keys, so a legacy raw prefix gap cannot hide a known admitted transfer. Scrubbed bytes are not re-signed or assigned a new source commitment. Original association survives legitimate ciphertext removal and can be reported as redacted. Unsupported historical packets without verified mapping do not gain fabricated provenance or replay permission.

## Storage, privacy and ordering

Private verified source refs, full-envelope proof and automatic application phase are durable internal VaultLog metadata for memory, SQLite and ReDB. Public Generic VaultLog clones remove the entire private collection and phase for every actual generic tail slot, including direct gateway responses. Public JSON, Debug and TRACE must not expose proof, base/target phase fields, signatures, original encrypted bodies, raw shares, keys or internal errors. Public observation has selected IDs/hash/status only; producer metadata does not retain an encrypted packet.

All affected Vault contexts are inspected before maintenance. Contradictory prepared owners fail closed before effects. Existing B2 initial-intake and private Recovery repair retain their gates; automatic Delete follows Recovery repair and precedes existing Removal repair. Direct reads outside the ordered ServerApp owner reject an unfinished automatic phase instead of exposing mixed state. Failed cleanup/readback keeps PurgePending and prevents ready/acknowledged presentation.

The separately owner-approved technical Noop and bounded client ownership/lifetime handling are implemented under their own [transport](device-transport-noop.md) and [private preparation](guarded-preparation.md) contracts. This producer-correlation contract grants no additional history healing, cancellation, signing, approval policy or transport replay exception. Full Core build/review/coverage, generated browser/native packages and real UI E2E remain separate gates; server target checks and host component evidence do not establish them.

## Interrupted producer raw save

The producer finalizer now runs existing canonical tombstone cleanup/readbacks after raw persistence and before invalidation/ACK. On ordinary ordered READ, after existing Recovery/automatic Delete/Removal repair, canonical tombstones plus exact verified retained source keys repair interrupted cleanup before data, ordinary tail or atomic observation. No extra journal/signature/command is used; old Event replay still rejects. The hook does not pre-scrub an unprepared guarded WRITE before it reserves its source proof.

Producer observation requires current canonical Member with matching full UserData/key binding. Removed or mismatched context gets fixed unavailable metadata rather than a subject/outcome; the query remains unsigned and is not key-possession authentication.

Direct gateways outside the ordered owner use a pure cleanliness guard for both retained raw packets and actual canonical VaultLog history. The guard and mutating cleanup share the same tombstone scrub predicate, including canonical requests/bindings/prepared updates/updates. A dirty clone differs, so the direct read is withheld without a write or payload projection. Ordered ServerApp reads perform full cleanup under their existing owner; original source proof and a still-absent raw slot remain unchanged. This behavior has an actual signed raw-before/direct-read regression; final independent source recheck remains separate from package/UI readiness.
