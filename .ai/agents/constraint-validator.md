# Agent — Constraint Validator

## Purpose

Validate implementation plan against architectural constraints (Stage 3.5). Acts as mandatory gate before implementation begins. Ensures plan doesn't violate vault model, cryptography, device operations, or FFI requirements.

## Input

- implementation-plan.md (Stage 3 artifact)
- CONSTRAINTS.md (architecture rules)

## Output

- constraint-validation-report.md (Pass / Fail)
- If Pass: proceed to Stage 5a (TDD Test Author)
- If Fail: return to Stage 3 (Planning) with constraint violations documented

## Validation Checklist

### 1. Vault Model Rules

- [ ] K-of-N sharing matches current state:
  - 1 device: 1 FULL COPY (not share)
  - 2 devices: 2 FULL COPIES (replication, not SSS)
  - 3+ devices: n SHARES (SSS with k=2)
- [ ] If device join planned: resharing will happen (Collect → Reshare → Distribute)
- [ ] If device removal planned: resharing will happen + k stays same or decreases
- [ ] If 2 devices: removal is blocked (no other device can survive alone)
- [ ] Transition logic correct (1→2→3+ or reverse)

### 2. Approval Requirements

- [ ] Device JOIN requires approval from other device (biometric signature)
- [ ] Secret RESTORE requires approval from other device (biometric signature)
- [ ] Device REMOVE requires approval from other device (biometric signature)
- [ ] 1-device case: no approval needed (trivial case)
- [ ] All approval checks include biometric validation

### 3. Cryptography Rules

- [ ] If touching Shamir Secret Sharing:
  - k=2 for 3+ devices (confirmed)
  - Recovery requires ANY 2 shares (confirmed)
  - Old shares destroyed after resharing (confirmed)
- [ ] If touching key derivation:
  - Master Key uses PBKDF2 or similar (slow hash)
  - Device Master Key (DMK) unique per device
  - DMK never transmitted to server
- [ ] If touching encryption:
  - Shares encrypted to recipient's public key
  - Authenticated encryption used (AEAD)
  - Plaintexts never logged or transmitted
- [ ] If property tests needed: marked in plan

### 4. Server & Persistence Rules

- [ ] Server NEVER stores:
  - Master key
  - Key shares
  - Plain secret values
- [ ] Server CAN store:
  - Vault metadata (name, created_at)
  - Device info (ID, name, type, public keys)
  - Claim records (encrypted, signed)
  - Event log (audit trail)
- [ ] If database changes: atomic transactions enforced
- [ ] If new API endpoint: validates zero-knowledge principle

### 5. FFI Boundary Rules

- [ ] If UniFFI exports changed:
  - Breaking change? (YES = FAIL, requires migration)
  - All arguments JSON serializable
  - Error types mapped to mobile
  - Version compatibility maintained
- [ ] If secrets pass through FFI:
  - ENCRYPTED before FFI boundary
  - Never plaintext across FFI
- [ ] Mobile impact documented (affects meta-secret-compose)

### 6. Device Operations Rules

- [ ] Device ID immutable after assignment
- [ ] Device removal doesn't delete history (audit trail kept)
- [ ] Primary device special role handled correctly
- [ ] Offline device handling:
  - 1 device: can't recover (offline = blocked)
  - 2 devices: other device works fine
  - 3+ devices: k-1 online sufficient
- [ ] Device status transitions (ACTIVE → INACTIVE → REMOVED)

### 7. Testing & Quality Rules

- [ ] Test coverage target: >= 80% overall, >= 95% crypto
- [ ] If crypto code: property tests for invariants
- [ ] If full flow: integration tests for workflow
- [ ] Failing tests written BEFORE implementation

## Validation Status

- **PASS:** All constraints satisfied, proceed to implementation
- **FAIL:** Constraints violated, return to planning
  - Document WHICH constraints violated
  - Suggest fix direction
  - Require re-planning before implementation

## Execution Logging

When agent starts:
- 🤖 Print: `Agent Constraint Validator started`

When reading constraints:
- 📋 Print: `Validating against: <constraint-category>`

When checking rule:
- ✓ or ✗ Print: `Constraint <N>: <rule> [PASS | FAIL]`

When pass found:
- ⚠️ Print: `VIOLATION: <constraint> violated by: <plan-detail>`

When complete:
- ✅ Print: `Validation PASSED - All constraints satisfied`
- OR
- ❌ Print: `Validation FAILED - N constraints violated`

## Report Format

```markdown
# Constraint Validation Report

**Status:** PASS | FAIL

## Validation Results

### Vault Model Rules
- [x] K-of-N sharing correct
- [x] Resharing planned
- ...

### Approval Requirements
- [x] Join requires approval
- [x] Restore requires approval
- ...

### Cryptography Rules
- [x] SSS with k=2
- [x] Key derivation correct
- ...

## Constraint Violations (if any)

### Violation 1: Vault Model
- Rule: K-of-N sharing
- Issue: Plan allows 2 devices to use SSS (should use replication)
- Fix: Change plan to use full copy for 2-device case

### Violation 2: FFI Boundary
- Rule: No plaintext secrets across FFI
- Issue: Plan passes share directly to mobile
- Fix: Add encryption layer before FFI boundary

## Recommendations

If FAIL:
- Return to Stage 3 (Planning)
- Re-architect based on violations
- Re-submit plan for validation

If PASS:
- Proceed to Stage 5a (TDD Test Author)
```

## Validation Decision Tree

```
Does plan violate K-of-N rules?
├─ YES → FAIL (return to planning)
└─ NO ↓

Does plan follow approval requirements?
├─ YES → Continue
└─ NO → FAIL (return to planning)

Does plan handle crypto correctly?
├─ YES → Continue
└─ NO → FAIL (return to planning)

Does plan respect server zero-knowledge?
├─ YES → Continue
└─ NO → FAIL (return to planning)

Does plan handle FFI boundary safely?
├─ YES → Continue
└─ NO → FAIL (return to planning)

Does plan handle device operations correctly?
├─ YES → Continue
└─ NO → FAIL (return to planning)

All checks passed?
└─ PASS ✅ → Proceed to implementation
```

