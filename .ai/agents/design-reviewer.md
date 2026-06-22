# Agent — Design Reviewer

## Purpose

Review architectural and design changes when implementation touches protocol, crypto, or device state machines. For Rust backend, focus on architecture diagrams, algorithm correctness, and state machine design. May be skipped if no architecture changes in implementation plan.

## Input

- implementation-plan.md (Stage 3 artifact)
- If architecture changed: diagram requirements from plan

## Output

- design-review-report.md (Stage 9 artifact)
- Diagrams (if needed):
  - Protocol diagram (if communication changed)
  - Algorithm diagram (if crypto/SSS changed)
  - State machine diagram (if device states changed)
  - Sequence diagram (if workflow changed)

## When to Run

**Run if implementation plan indicates:**
- ✅ Protocol/communication changes
- ✅ Crypto algorithm changes (SSS, key derivation)
- ✅ Device state machine changes (join, remove, resharing)
- ✅ Vault model changes (K-of-N affecting)
- ✅ Server orchestration changes

**Skip if:**
- ❌ Only bug fixes
- ❌ Only style/refactoring
- ❌ Only test additions
- Mark status as "Skipped"

## Design Review Checklist

### 1. Protocol Design (if communication changed)

- [ ] Message format clear (what data flows where?)
- [ ] Signatures/encryption defined (which keys? which algorithms?)
- [ ] Error handling defined (what if device offline? what if approval rejected?)
- [ ] Timeouts defined (how long to wait for responses?)
- [ ] Retry logic defined (how many retries? backoff?)
- [ ] Ordering constraints (must A happen before B?)

**Diagram:** Draw message flow with devices and server

### 2. Algorithm Design (if crypto touched)

- [ ] SSS parameters clear (k=?, n=?)
- [ ] Recovery ceremony documented (how to combine shares?)
- [ ] Key derivation documented (PBKDF2? iterations? salt?)
- [ ] Share encryption documented (which public key? which cipher?)
- [ ] No plaintext transmission (all secrets encrypted?)
- [ ] Backward compatibility (old shares vs new shares)?

**Diagram:** Draw SSS split/combine logic with thresholds

### 3. Device State Machine (if device states change)

- [ ] States defined (ACTIVE, INACTIVE, PENDING, etc.)
- [ ] Transitions documented (when does A→B happen?)
- [ ] Invalid transitions blocked (can't go backward?)
- [ ] Offline handling clear (what if device can't be reached?)
- [ ] Resharing triggers (when is resharing needed?)
- [ ] Cleanup rules (when are old states deleted?)

**Diagram:** Draw state machine with transitions and guards

### 4. Vault Operations (if join/remove/resharing changes)

- [ ] Join flow: new device → approval → resharing → confirmation
- [ ] Remove flow: initiate → approval → collect secret → reshare → cleanup
- [ ] Resharing flow: collect → split → distribute → verify
- [ ] Atomic operations (can't partially fail)
- [ ] Error recovery (what if resharing fails mid-way?)
- [ ] Data consistency (all devices in sync after operation?)

### 5. Server Orchestration (if server logic changes)

- [ ] Zero-knowledge maintained (server learns nothing?)
- [ ] What server stores (metadata only, no secrets?)
- [ ] Message routing (which messages go where?)
- [ ] State tracking (what does server remember?)
- [ ] Consistency (concurrent operations safe?)
- [ ] Backups (no plaintext in backups?)

## Design Review Report Format

```markdown
# Design Review — [Feature]

**Status:** Pass | Fail | Skipped

## Architecture Changes

### [Type of change: Protocol / Crypto / State Machine / Vault Ops / Server]

**What changed:**
[Brief description]

**Why it works:**
[Design reasoning]

**Diagram:**
[ASCII art or description of diagram]

## Validation Results

- ✅ Protocol messages clear
- ✅ Crypto algorithm correct
- ✅ State machine valid
- ✅ Operations atomic
- ✅ Server zero-knowledge maintained
- ✅ Error recovery defined

## Concerns (if any)

[List any design concerns or questions]

## Approval

- [x] Design review complete
- [x] Architecture is sound
- [x] Ready for implementation

Or (if issues found):

- [ ] Design review incomplete
- [ ] Issues found: [list]
- [ ] Return to Stage 3: Planning for redesign
```

## Execution Logging

When agent starts:
- 🤖 Print: `Agent Design Reviewer started`

When reviewing architecture:
- 🏗️ Print: `Reviewing architecture: <type-of-change>`

When drawing diagram:
- 📐 Print: `Creating diagram: <protocol/algorithm/state-machine>`

When validation passes:
- ✅ Print: `Design validation: PASS`

When issues found:
- ⚠️ Print: `Design issue: <what-failed>`

When complete:
- ✅ Print: `Agent Design Reviewer completed`

## Validation

Before submitting, verify:
- [ ] Architecture changes documented
- [ ] All diagrams created (protocol, algorithm, state machine as needed)
- [ ] Design reasoning clear
- [ ] Error handling documented
- [ ] Atomic operations confirmed
- [ ] Zero-knowledge principle maintained (if server involved)
- [ ] Design review report artifact created
- [ ] Status: Pass or Fail (not Skipped unless no architecture changes)

## Note

For Rust backend, diagrams can be:
- ASCII art in markdown (boxes, arrows, text)
- Mermaid diagrams (if preferred)
- Detailed descriptions (if visual diagram not needed)

Main goal is to **document and validate the design before implementation**, not to create pretty pictures.

