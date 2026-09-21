# Architecture Constraints — meta-secret-core

Mandatory architectural rules for the Rust backend cryptography and protocol implementation. All code changes must validate against these constraints.

**Last updated:** 2026-09-20
**Maintainer:** Architecture Guardian (validate at Stage 3.5)

---

## Quick Reference

| Constraint | Rule | Scope |
|---|---|---|
| **K-of-N Sharing** | 1 device→k=1 (trivial); 2 devices→k=1 (full replication); 3 devices→k=2 (SSS) | Core |
| **Device Limit** | A Vault may contain at most 3 devices. Core rejects a fourth and later join. | Membership |
| **Redistribution** | Required on every device add/remove | Vault ops |
| **Approval Required** | JOIN, RESTORE SECRET, DELETE DEVICE need biometric signature | Consensus |
| **Atomicity** | Collect→Reshare→Distribute is all-or-nothing | Transactions |
| **No Server Plaintext Storage** | The server never stores plaintext Master Keys, Key Shares, or Secrets. It may temporarily queue an Encrypted Key Share while delivery/synchronization is pending, then remove it after delivery. | E2E principle |
| **Two Cannot Erase Each Other** | With 2 devices, neither can remove the other | Safety |
| **Recovery Status Ownership** | Core alone computes recovery lifecycle and `clientStatus`; UI only executes the resulting instruction | Recovery |
| **Shared Logic Ownership** | Core alone owns cryptography, Key Share operations, quorum, claims, recovery decisions, JOIN/DELETE/resharing, synchronization, conflicts, and Secret reveal eligibility; clients only adapt and render Core results | Cross-platform architecture |
| **First Response Wins** | The first server-processed receiver decision terminalizes the recovery claim; a late opposite decision is ignored | Recovery consensus |
| **Reconnect Flush** | A client must upload locally queued recovery decisions before reading refreshed canonical state after connectivity returns | Offline recovery |
| **Sensitive Logging** | Never log Master Keys, plaintext Secrets, Key Shares (Доли), encrypted Key Shares, or recovery material in debug or production builds. Log only opaque IDs, statuses, and counts. | Security |
| **Database Filename** | Mobile local databases use `meta-secret-db-<SHA-256(master_key)>.db` with lowercase hexadecimal digest; the raw Master Key is never part of a filename or path log. | Device storage |

---

## 1. Vault Model & Device States

### 1.0 Recovery Status Ownership

For every `Recover` claim, core computes the per-device `clientStatus` from the
claim state and the current device identity. This is the only authority for whether
a recovery is active, ready, declined, or complete.

- Web, iOS, and Android UI must react to `clientStatus`; they must not infer
  lifecycle state or implement K-of-N/quorum rules. They may technically choose
  one claim from the set already marked `NeedApprove` by core.
- The UI may retain a claim ID only to deduplicate an alert and submit the user's
  approve/decline decision.
- `Done` is emitted only after the sender has recovered the secret and the
  completion has been persisted. Receivers close recovery alerts only then.
- A receiver that has already approved or declined has no further action until
  completion; this is represented by no client instruction (`None`).
- Recovery uses first-response-wins ordering: the first receiver decision
  processed by the server is authoritative for the claim.
- If the first decision is `Decline`, remaining pending receivers become
  terminally declined and the sender cannot recover or reveal the secret.
- If the first decision is `Approve`, the approving receiver reaches
  `Sent`/`Delivered`, recovery may proceed once the threshold is met, and a
  later `Decline` cannot revoke that result.
- Terminal receiver statuses are monotonic and cannot be reverted by stale
  snapshots or device-log reconciliation.
- A recovery decision created while offline must be flushed to the server before
  the client treats a reconnect state refresh as authoritative. The Web client
  performs an explicit idempotent reconnect sync; this is a state boundary, not
  a timer-based retry.
- “First” is a server-processing guarantee: for packets that arrive at nearly
  the same time, the ordered event stream decides which one is first; it is not
  a wall-clock promise.
- Accepting a new vault member invalidates every still-pending `Recover` claim
  created for the previous membership set. The server records those pending
  receiver decisions as `Declined` and publishes an `SsClaims` invalidation;
  existing `Sent`/`Delivered` decisions remain unchanged.

### 1.0.1 Shared Logic Ownership

Rust Core is the only authority for shared security and domain decisions:

- cryptography and Key Share operations;
- K-of-N/quorum calculations;
- recovery claims and lifecycle statuses;
- Approve/Decline and first-response-wins;
- JOIN, DELETE DEVICE, and resharing;
- synchronization, conflict resolution, and Secret reveal eligibility.

Web, CLI, Android, and iOS clients may call Core, map Core responses into
client models, coordinate ViewModels and platform adapters, and render UI. They
must not reimplement these decisions. They may perform non-security UX
validation (for example, reject an empty form field) and invoke platform UI
such as biometry or navigation. A client may choose presentation (for example
`Recover` versus `Show`) only from the status returned by Core.

### 1.1 K-of-N Principle (Adaptive Sharing)

**State: 1 Device**
```
- Configuration: n=1, k=1 (trivial)
- Storage: 1 FULL COPY of master key
- Recovery: Trivial (only option)
- Resharing: Never needed
- Offline handling: Device down = vault inaccessible
```

**State: 2 Devices**
```
- Configuration: n=2, k=1 (full replication — each device has complete secret)
- Storage: 2 FULL COPIES of master key (each device stores identical complete key)
           Device A: FULL SECRET (encrypted at rest)
           Device B: FULL SECRET (encrypted at rest)
- Recovery: EITHER device can independently recover secret (no ceremony needed)
            Each device holds complete, standalone copy
- Offline handling: 1 device offline = OK (other device works independently)
                   Device loss = other device can recover all secrets
- Rationale: Trade-off for better UX — if one device is lost/damaged,
            user can still access all secrets from remaining device.
            More practical than SSS for 2-device vaults.
```

**State: 3 Devices (maximum)**
```
- Configuration: n=3, k=2 (Shamir Secret Sharing)
- Storage: 3 SHARES, each device holds exactly its own share
           Each share is cryptographically secure chunk
- Recovery: Any 2 devices can combine shares → recover secret
            Requires one other device's approval and share
- Offline handling: 1 device offline = OK (k-1 others available)
                   2+ devices offline = recovery impossible
- Membership: a fourth device is rejected until a future redistribution protocol is designed
```

### 1.2 Redistribution on Device Join (Addition)

#### Recovery claims during a join

If a `Recover` claim is pending when a new member is accepted, the claim is
bound to the old member set and must not remain actionable. The server first
terminalizes all of its pending receiver statuses as `Declined`, then publishes
the normal claims invalidation. Membership redistribution creates a new
`Split` claim for the joining device. Clients must remove the stale recovery
alert and must not show the old recovery request to the new member.

**Flow: 1 device (A) + new device B joins**

```
Initial: Device A has 1 FULL COPY of master_key (n=1, k=1)

Step 1: COLLECT
  - B sends join request with device info
  - A receives join request
  
Step 2: APPROVAL (REQUIRED)
  - A's user performs BIOMETRIC + CONSENT on device A
  - A signs approval message
  
Step 3: RESHARE (Full Replication for 2 devices)
  - A prepares FULL COPY for replication (no SSS splitting)
  - Configuration: n=2, k=1 (each device has complete secret)
  - A sends complete secret (encrypted to B's public key) → B
  
Step 4: CONFIRM & STORE
  - B decrypts and stores FULL COPY locally
  - B confirms receipt and decryption success
  - A keeps its original FULL COPY (not replaced!)
  
Result: A=FULL COPY, B=FULL COPY — independent vaults, identical secrets
  
✅ BOTH DEVICES INDEPENDENT:
  - Each device can independently recover all secrets
  - No ceremony needed
  - Either device going offline = OK
  - Device loss = other device has complete backup
```

**Flow: 2 devices (A, B) + new device C joins**

```
Initial: A=FULL COPY, B=FULL COPY (n=2, k=1 full replication)

Step 1: COLLECT (on initiating device, e.g., A)
  - C sends join request
  - A and B receive request
  
Step 2: APPROVAL (REQUIRED on OTHER devices)
  - B's user performs BIOMETRIC + CONSENT on device B
  - B signs approval message, sends to A
  
Step 3: COLLECT SHARED SECRET (on A)
  - A has its FULL COPY (either A's or B's copy works)
  - A uses local copy (no need to contact B for this)
  - Secret in memory: complete, unencrypted
  
Step 4: RESHARE (SSS - switching from replication to shares)
  - A runs Shamir Secret Sharing: split secret into 3 shares
  - Configuration: n=3, k=2
  - Shares: s1 (for A), s2 (for B), s3 (for C)
  
Step 5: DISTRIBUTE & REPLACE
  - A: REPLACE its FULL COPY with s1 (cryptographic share)
  - A sends s2 (encrypted to B) → B REPLACES FULL COPY with s2
  - A sends s3 (encrypted to C) → C stores s3 (first time)
  - All devices confirm receipt and decryption
  
Result: A=SHARE (s1), B=SHARE (s2), C=SHARE (s3)
  Recovery requires: ANY 2 devices together

⚠️ CRITICAL: OLD FULL COPIES on A and B are DESTROYED
  - Vault transitions from independent 2-device model to dependent 3-device SSS model
  - Recovery now requires ceremony (biometric approval + device coordination)
```

**Flow: 3 devices (A, B, C) + new device D joins**

```
Initial: A=SHARE, B=SHARE, C=SHARE (SSS state, k=2)

Result: D's join is rejected by Core with "Vault supports at most 3 devices".
No new share is generated and the existing three-device state is unchanged.
```

### 1.3 Redistribution on Device Removal

**Flow: 3 devices (A, B, C) + remove C**

```
Initial: A=SHARE, B=SHARE, C=SHARE (n=3, k=2)

Step 1: INITIATE REMOVAL (on A)
  - A's user requests "Remove device C"
  - A sends removal request to B and C
  
Step 2: APPROVAL (REQUIRED on OTHER devices)
  - B's user performs BIOMETRIC + CONSENT on device B
  - B signs approval message, sends to A
  - C receives removal request but cannot approve it
  
Step 3: COLLECT SHARED SECRET (on A)
  - A has SHARE s1
  - B sends SHARE s2 after approval
  - A combines s1 + s2 → recover complete secret
  - Secret is ONLY in A's memory (ephemeral)
  
Step 4: RESHARE (on A)
  - A runs SSS: split secret into 3 shares
  - Configuration: n=3, k=2
  - Shares: s1' (for A), s2' (for B), s3' (for C - will be deleted)
  
Step 5: DELETE & REDISTRIBUTE
  - A marks C as INACTIVE (cannot receive messages)
  - A REPLACES its SHARE: s1 → s1'
  - A sends s2' (encrypted to B) → B REPLACES s2 with s2'
  - A CONFIRMS C is deleted (no share sent to C)
  - A notifies B that removal is complete
  
Step 6: CONFIRMATION (on B)
  - B confirms it received new share s2'
  - B marks C as INACTIVE in its vault state
  
Result: A=FULL COPY, B=FULL COPY (only 2 devices remain)
        C is INACTIVE (cannot participate in claims)

⚠️ CRITICAL: Old shares on A and B are DESTROYED
```

**Flow: 2 devices (A, B) + remove B**

```
Initial: A=FULL COPY, B=FULL COPY (n=2, k=1 full replication)

Step 1: INITIATE REMOVAL (on A)
  - A's user requests "Remove device B"
  
Step 2: APPROVAL (REQUIRED but special case)
  - Only A and B exist
  - B cannot approve removal of itself
  - A cannot collect secret without B's permission
  - ⚠️ REMOVAL IS BLOCKED
  
UI: B's removal button is HIDDEN (not shown to user)
    User cannot remove B (system prevents it)

Result: A=FULL_COPY, B=FULL_COPY (both remain intact)

⚠️ CRITICAL RULE: 
  With exactly 2 devices, neither can remove the other.
  Removing one = other has no way to recover secret.
  This is a safety guardrail (UI level prevents it).
```

### 1.4 K Value Rules

**Current policy:**

| n | k | Notes |
|---|---|---|
| 1 | 1 | Trivial — full copy |
| 2 | 1 | Full replication — either device has the complete secret |
| 3 | 2 | Any 2 of 3 can recover |

The supported maximum is three devices. For the three-device state the threshold is
`k=2`; it is not `k=n-1`.

**Implementation:** `SharedSecretConfig::calculate()` in
`meta-secret/core/src/secret/data_block/common.rs`

**Removal examples:**
```
n=3 (k=2) → remove 1 → n=2 (k=1) ✅ reshare to full replication
n=2 (k=1) → remove 1 → n=1 (cannot remove — blocked by UI)
```

### 1.5 Transitions & State Changes

**State Transitions**

| From | To | Trigger | Shares Change |
|---|---|---|---|
| 1 device (k=1) | 2 devices (k=1) | Device join | 1 COPY → 2 full copies |
| 2 devices (k=1) | 3 devices (k=2) | Device join | 2 full copies → 3 SSS shares (reshare) |
| 3 devices (k=2) | 2 devices (k=1) | Device removal | 3 SSS shares → 2 full copies (reshare) |
| 2 devices (k=1) | 1 device | Cannot happen | (blocked by UI) |
| 3 devices (k=2) | 4 devices | Cannot happen | (blocked by Core device limit) |

---

## 2. Approval & Consensus Rules

### 2.1 Actions Requiring Approval

**All state-changing vault operations require approval from OTHER devices:**

1. **JOIN NEW DEVICE**
   - Initiator: New device (or existing device on behalf of new device)
   - Approval needed: From 1 other existing device (biometric)
   - Flow: Request → Biometric on other device → Approval sent → Reshare/Replicate

2. **RESTORE SECRET (Recover from Shares)**
   - Initiator: Any device in vault
   - Approval needed: From 1 other device (biometric) to combine shares
   - Threshold: k-1 other devices must approve (currently 1 for k=2)
   - Why: Ensures secret recovery is witnessed by multiple parties

3. **DELETE DEVICE**
   - Initiator: Owner or any device
   - Approval needed: From 1 other device (biometric)
   - Exception: Cannot delete last 2 devices (UI blocks)
   - Flow: Request → Biometric on other device → Collect secret → Reshare

### 2.2 Approval Mechanism

**Biometric approval and signature (required protocol property):**
- User performs biometric (fingerprint, face) on OTHER device
- System creates an approval message containing action type, vault/Claim or request ID, device IDs, and freshness data (nonce or timestamp)
- Message MUST be cryptographically signed by the approving device's private key

**Transport and verification:**
- Approval message is sent through the sync transport
- The server MUST verify the signature against the registered public key and authorization before mutating canonical state
- The recipient may verify it again before secret collection
- Implemented contract: Core wraps state-changing writes and recovery completion in
  `SignedAction`; the server resolves the registered DSA public key, verifies the
  canonical payload, checks the signer/target relationship, and rejects the
  command before persistence when any check fails.
- Every action stream carries a monotonic nonce. The server compares it with the
  durable event sequence (and terminal recovery state for completion), so an old
  or replayed command cannot be applied twice.
- Unsigned write events and unsigned recovery completions are invalid protocol
  messages. DELETE DEVICE remains a separate implementation task.

**No Approval Needed:**
- 1 device vault (no other device to approve)
- User creates new secret locally (no resharing needed)

### 2.3 Quorum Rules

**For 3 devices:**
```
Approval threshold: 1 device (simple majority path)
Rationale: k=2 means any 2 can recover secret
          1 other device approving = we have 2 total (initiator + approver)

Future: May change to 50% or 66% threshold
        But currently: 1 approval sufficient
```

**For 2 devices:**
```
Approval: Not possible to get from "other" for removal
         (would leave only 1 device)
         UI blocks removal button
```

**For 1 device:**
```
No approval needed (trivial case)
```

---

## 3. Secret Sharing & Distribution

### 3.1 Share vs. Full Copy Logic

**Full Copy (1-2 device cases):**
- Complete, unencrypted master key (encrypted at rest only)
- Can independently recover secrets
- No ceremony needed
- Two copies are independent (no sync requirement)

**Share (3-device case):**
- Cryptographic share from SSS
- Cannot independently recover secret
- Requires k other shares
- Must be combined with other shares via ceremony

**Storage Format:**
```
Full Copy: { key_material: bytes, encrypted: true }
Share:     { share_id: u32, share_data: bytes, encrypted: true }
```

### 3.2 Resharing Process (Atomic)

**Three steps that MUST be atomic (all-or-nothing):**

1. **COLLECT**
   - Gather shares/copies from multiple devices
   - Return: complete secret (in memory)
   - Requires: approval signatures from other devices

2. **RESHARE**
   - Run SSS to generate new shares from collected secret
   - Input: complete secret, n (device count), k (threshold)
   - Output: n new shares
   - Timing: Happens on initiating device's memory

3. **DISTRIBUTE & REPLACE**
   - Encrypt each new share to target device's public key
   - Keep remote Encrypted Key Shares only as temporary outbound workflows
   - Send each new share to its target device
   - Each device REPLACES old share/copy with its own new share
   - After the server accepts an outbound workflow, delete that remote share from
     the sender's local database
   - The sender retains only its own local share; it must not retain other devices'
     shares after synchronization

**Atomicity Guarantee:**
- If any device fails to confirm receipt: ABORT entire operation
- Failed devices: old shares remain valid, new shares discarded
- No partial updates (no device left with mismatched shares)

### 3.3 Recovery Rules Per Device Count

| Devices | Mechanism | Approval | Ceremony |
|---|---|---|---|
| 1 | Direct (FULL_COPY) | None | None |
| 2 | Direct (FULL_COPY) | None | None |
| 3 | SSS combine | From 1 other (k-1) | Yes: sign + verify |

---

## 4. Device Management

### 4.1 Device Identity

**Device ID:**
- 32-byte unique identifier (immutable)
- Assigned during first registration
- Never changes (even if device is re-registered)
- Used for: targeting shares, claim addressing, audit log

**Device Data:**
```
{
  id: [u8; 32],                // immutable identifier
  name: String,                // mutable (user can change)
  device_type: DeviceType,     // iOS, Android, Desktop, etc.
  public_keys: {
    dsa_pk: Vec<u8>,           // for signing
    transport_pk: Vec<u8>,     // for encryption
  },
  status: DeviceStatus,        // ACTIVE, INACTIVE, REMOVED
  created_at: Timestamp,       // when registered
  primary: bool,               // true if vault initiator
}
```

### 4.2 Device Lifecycle

**Registration (on join):**
1. New device sends join request with ClientDeviceInfo
2. Existing device approves (biometric)
3. Assign unique Device ID
4. Store device info in vault
5. Generate share or full copy for new device
6. Mark as ACTIVE

**Removal (on delete):**
1. Initiating device requests removal
2. Other device approves (biometric)
3. Collect secret from initiating device
4. Reshare to remaining devices
5. Mark removed device as INACTIVE
6. Old shares are destroyed, new shares distributed

**Offline:**
1. Device offline but still ACTIVE in vault
2. Claims can proceed with k-1 devices
3. When device comes online: receives pending claims via socket

### 4.3 Primary Device Concept

**Primary Device:**
- The first device that created the vault
- Marked with `primary: true` in device data
- Special role: (currently minimal, may expand)
- Can be removed like any other device

**Secondary Devices:**
- Any device added after vault creation
- No special privileges
- Can initiate actions (join, restore, remove) like any device

---

## 5. Server & Persistence

### 5.1 Zero-Knowledge Principle

**Server NEVER stores in plaintext:**
- Master Key (any format)
- Key Shares
- Complete Secrets
- Private Keys

The server may temporarily queue an Encrypted Key Share while delivery is
pending because it cannot decrypt the ciphertext without the recipient's
Private Key. After the recipient synchronizes, the workflow item is removed; it
is not part of the server's durable Vault data. This is transport buffering of
ciphertext, not server possession of a usable Key Share.

**Server CAN store:**
- Vault metadata (name, created_at, device_count)
- Device info (ID, name, type, public keys, status)
- Claim records (metadata) and temporary Encrypted Key Shares pending delivery
- Event metadata and, once implemented, verified signed audit events

### 5.2 Encrypted At Rest

**Secrets in transit:**
- Encrypted to recipient's public key (asymmetric)
- Signed by sender (non-repudiation)

**Secrets stored on device:**
- Encrypted with Device Master Key (symmetric)
- Additional layer: DB encryption at rest (optional)

### 5.3 Atomic Transactions

**Operations that MUST be atomic:**
1. Device join + resharing
2. Device removal + resharing
3. Claim approval + delivery confirmation
4. State transitions

**Implementation:**
- Use database transactions (SERIALIZABLE isolation)
- All-or-nothing: commit only if all steps succeed
- Rollback if any step fails

---

## 6. Security Properties

### 6.1 End-to-End Encryption

**Principle:**
- Secrets encrypted from device to device
- Server cannot decrypt
- Encrypted Key Shares transmitted over socket

**Implementation:**
- Asymmetric encryption: recipient's public key
- Symmetric encryption: shared session keys (if needed)
- Authenticated encryption: AEAD ciphers

### 6.2 Zero-Knowledge Proof

**Server learns nothing about:**
- Actual secret values
- Device master keys
- Vault structure (beyond device count)
- Recovery patterns

**Server can infer:**
- Number of devices
- Approximate timing of operations
- That a claim was made (not which secret)

### 6.3 Non-Repudiation

**Required protocol property — all critical actions must be signed and verified:**
- Device join approval
- Secret recovery approval
- Device removal approval
- Claim confirmation

**Signature verification:**
- The signed payload must bind the action to its vault, Claim/request ID, sender and receiver/device IDs, and freshness data
- Server verification is mandatory before applying JOIN, RESTORE SECRET, DELETE DEVICE, or Claim confirmation
- `SignedAction` is the common envelope for write events and RESTORE completion.
  Its canonical payload binds action type, Vault, Claim/request IDs, signer,
  sender/receiver IDs, stream, nonce, and the complete action body.
- A receiver signs its own Approve/Decline action. After the sender recovers and
  shows the Secret, the sender signs the terminal completion and names the
  receiver whose `Sent` decision supplied the recovery share. The server checks
  both the target receiver and the signer role before applying the completion.
- The server resolves the signer's registered DSA public key from canonical Vault
  membership (with only the explicit bootstrap JOIN exception), verifies the
  signature, enforces authorization, and advances the sequence before applying
  state.
- DELETE DEVICE is intentionally tracked separately and is not claimed as
  covered by this issue.

---

## 7. UniFFI & Mobile Binding

### 7.1 FFI Boundary

**All FFI arguments and returns are JSON strings:**
```rust
#[uniffi::export]
pub fn collect_secret(vault_json: String) -> String {
    // Input: JSON serialized Vault
    // Output: JSON serialized Secret or Error
}
```

**Why JSON:**
- Language-agnostic serialization
- Easy debugging
- Version-stable format

### 7.2 Backward Compatibility

**FFI signature NEVER changes without migration:**
- New parameters: add as optional JSON field
- Removed parameters: keep for backward compat, ignore if present
- Changed types: serialize both old and new, handle gracefully

**Version check:**
- Mobile queries `core_version()` on startup
- If versions mismatch: show upgrade prompt to user
- Never silently fail due to version mismatch

### 7.3 Error Propagation

**All errors returned as JSON:**
```json
{
  "success": false,
  "error": {
    "code": 4001,
    "message": "Share decryption failed"
  }
}
```

**Mobile error handling:**
- Parse error code
- Translate to user-friendly message
- Log error with context

---

## 8. Testing & Quality

### 8.1 Unit Tests (Crypto)

**All crypto operations must be tested:**
- Master Key generation and storage
- Share generation (SSS)
- Share combination (SSS)
- Encryption/decryption
- Signature generation/verification

**Coverage:** ≥ 95% for crypto modules

### 8.2 Integration Tests (Flows)

**Full vault flows tested:**
1. **1-device vault:**
   - Create vault
   - Add secret
   - Restore secret

2. **2-device vault (replication):**
   - Device A creates vault
   - Device B joins (replication)
   - Either device can restore secret
   - Device A removes B (blocked by UI)
   - Device B removes A (blocked by UI)

3. **3-device vault (SSS):**
   - Device A creates vault
   - Device B joins (still replication)
   - Device C joins (transition to SSS)
   - All devices have shares
   - Any 2 devices can restore secret
   - Device removal + resharing
   - Device add + resharing

4. **Approval flows:**
   - Join requires approval
   - Restore requires approval
   - Remove requires approval
   - Biometric signature verification

### 8.3 Property Tests

**SSS properties:**
- For any n devices and k threshold
- Collect any k shares + combine = original secret
- Reveal any k-1 shares = no information about secret

---

## 9. Cross-Project Rules (compose ↔ core)

### 9.1 FFI Contract

**Mobile app depends on these FFI exports:**
- `generate_master_key()` → JSON
- `prepare_sign_up(vault_name)` → JSON
- `continue_sign_up()` → JSON
- `restore_secret(pass_id)` → JSON (with approval)
- `add_device(device_info)` → JSON (with approval)
- `remove_device(device_id)` → JSON (with approval)

**Breaking changes not allowed without migration path**

### 9.2 Shared GLOSSARY

**Both projects use `.ai/GLOSSARY.md` terminology:**
- "Share" = cryptographic share (not a social media share)
- "Device" = physical phone/desktop (not player device)
- "Vault" = encrypted container (not game vault)
- "Claim" = secret distribution or recovery request

### 9.3 Approval Consistency

**Required cross-project approval contract:**
- Mobile UI: shows biometric prompt on other device
- Core/server: must validate the cryptographic signature and authorization on approval
- Both: block actions without valid approval

**Current implementation status:** the UI prompt and Core signing primitives are
transported through the mandatory `SignedAction` envelope. The server resolves the
registered device key and verifies the signature, signer/receiver authorization,
and action sequence before accepting the approval. DELETE DEVICE remains a
separate follow-up task.

---

## 10. Validation Checklist (Stage 3.5)

Use this checklist when validating new code against constraints:

```
[ ] K-of-N logic matches current state (1/2/3 devices)
[ ] Redistribution triggered on device add/remove
[ ] Collect→Reshare→Distribute is atomic
[ ] Approval required before secret collection
[ ] Biometric signature verified
[ ] Two devices cannot remove each other (UI blocks)
[ ] Old shares/copies destroyed after redistribution; sender retains only its own share
[ ] Fourth-device join is rejected by Core
[ ] Server never stores plaintext keys, shares, or Secrets; only metadata and E2E ciphertext
[ ] All FFI returns are JSON strings
[ ] Error codes mapped to mobile messages
[ ] Non-repudiation: all actions signed
[ ] Device ID is immutable
[ ] State transition valid (see 1.5 table)
[ ] Test coverage >= 80% overall, >= 95% crypto
```

---

**Next:** See `.ai/commands/implement-issue.md` for full 11-stage workflow using these constraints as validation gate.
