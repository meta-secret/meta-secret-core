# Execution Checklist: Implement Issue Workflow (meta-secret-core)

**MANDATORY** reference for executing `implement issue <payload>` command.

---

## ⚠️ CRITICAL STAGES — Do NOT skip these under any circumstances:

### Stage 4: Constraint Validator (MANDATORY GATE)
- **Agent:** `constraint-validator`
- **Action:** Validate plan against CONSTRAINTS.md
- **Output:** `.ai/artifacts/run/MS-<run-id>-0035-constraints.md`
- **Status:** Pass / Fail
- **Non-negotiable:** This stage MUST be executed BEFORE implementation. If failed, return to Stage 3.
- **Failure action:** Return to Stage 3 (Planning) with constraint violation details

---

### Stage 7: Security Review (CRITICAL)
- **Agent:** Dedicated security reviewer
- **Action:** Crypto audit + unsafe code check + FFI boundary review
- **Output:** `.ai/artifacts/run/MS-<run-id>-007-security-review.md`
- **Status:** Pass / Fail
- **Checks:**
  - If crypto touched: audit SSS/key derivation correctness
  - If device logic touched: verify resharing protocol
  - If FFI changed: check no plaintext exposure
  - Run: `cargo clippy --all-targets -- -D warnings`
  - Audit: all `unsafe` blocks (justify each)
  - Check: no secret logging

**If Stage 7 fails:**
- Document security concerns
- Return to Stage 3 (Planning) with security notes
- May require design changes, not just code fixes

---

### Stage 8: Code Review (CRITICAL)
- **Agent:** `code-reviewer`
- **Action:** Review implementation against constraints, architecture, and coverage
- **Output:** `.ai/artifacts/run/MS-<run-id>-008-code-review.md`
- **Status:** Pass / Fail
- **Coverage Check:** Verify >= 80% minimum test coverage (if coverage < 80%, FAIL this stage)
- **Constraints Check:** Re-validate against `.ai/CONSTRAINTS.md`
- **FFI Check:** If boundary changes, note mobile compatibility
- **Non-negotiable:** This stage MUST be executed. If skipped, workflow is incomplete.

**If Stage 8 fails:**
- Return to Stage 3 (Planning) with failed artifact as input
- Re-run stages 5a, 5b, 5c (TDD cycle)
- Re-run Stage 6 (Build)
- Re-run Stage 7 (Security Review)
- Re-run Stage 8 (Code Review)

---

### Stage 9: Design Review (CONDITIONAL)
- **Agent:** `design-reviewer`
- **Condition:** Only run if Stage 3 plan indicates architecture changes
- **Action:** Create diagrams (Protocol, Algorithm, State Machine) if architecture changed
- **Output:** `.ai/artifacts/run/MS-<run-id>-009-design-review.md`
- **Status:** Pass / Failed / Skipped
- **If no architecture changes:** Mark status as "Skipped" (explicitly document why)
- **If architecture changes:** Must execute and must pass

**If Stage 9 fails:**
- Document design concerns
- Return to Stage 3 (Planning) with design review notes
- May require architecture redesign

---

### Stage 10: Coverage Verification (CRITICAL)
- **Agent:** `code-reviewer`
- **Action:** Execute `cargo tarpaulin --out Html --timeout 300 --fail-under 80`
- **Output:** `.ai/artifacts/run/MS-<run-id>-010-coverage.md`
- **Status:** Pass / Fail
- **Required metrics:**
  - Overall coverage >= 80%
  - Crypto modules >= 95% (preferred)
  - Report uncovered lines
- **Non-negotiable:** This stage MUST be executed. Coverage < 80% = FAIL
- **Even without Figma:** This stage still runs (Figma only affects Stage 9)

**If Stage 10 fails:**
- Document which lines/modules lack coverage
- Return to Stage 3 (Planning) to add tests
- Must re-run Stages 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10

---

### Stage 12: User Approval (MANDATORY)
- **Agent:** `release-manager`
- **Action:** STOP and ASK USER for approval before PR creation
- **Question:** "Should we proceed to Stage 13 (Branch + Commit + PR)?"
- **Output:** `.ai/artifacts/run/MS-<run-id>-012-approval.md`
- **Status:** Success (user approved) / Cancelled (user declined)
- **Non-negotiable:** Do NOT auto-commit or auto-create PR without user approval
- **If user declines:** Stop workflow, wait for further instructions

---

## Execution Flow — Summary

```
Stage 1 → Issue Coordinator
     ↓
Stage 2 → Requirements Clarifier (Grill Me)
     ↓
Stage 3 → Feature Planner
     ↓
Stage 3.5 → Constraint Validator ⚠️ GATE (Pass/Fail)
     ↓
BRANCH: TDD Implementation
     ├─ Stage 5a: TDD Test Author
     ├─ Stage 5b: TDD Implementer
     └─ Stage 5c: TDD Refactorer
     ↓
Stage 6 → Build (cargo build)
     ↓
VALIDATION BRANCH:
     ├─ Stage 7: Security Review ⚠️ CRITICAL (Pass/Fail)
     ├─ Stage 8: Code Review ⚠️ CRITICAL (Pass/Fail)
     ├─ Stage 9: Design Review (if architecture changed, else Skipped)
     └─ Stage 10: Coverage Verification ⚠️ CRITICAL (Pass/Fail)
     ↓
Stage 11 → Test Run (cargo test)
     ↓
USER APPROVAL GATE:
     └─ Stage 12: Ask User ⚠️ MANDATORY (Yes/No)
     ↓
Stage 13 → release-manager (Create PR)
```

---

## Failure Recovery

If any CRITICAL stage fails:
1. Identify root cause
2. Document in artifact with **Status: Failed**
3. Return to Stage 3 (Planning)
4. Create fix plan based on failure
5. Re-execute stages 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13
6. Max retries: 2 full loops

---

## Validation Checklist Before Stage 13

Before proceeding to Stage 13 (PR creation), verify ALL artifacts exist:

- ✅ Stage 1: Issue analyzed
- ✅ Stage 2: Clarifications completed
- ✅ Stage 3: Plan created
- ✅ Stage 3.5: Constraints validated (Status: Pass)
- ✅ Stage 5a: Failing tests written
- ✅ Stage 5b: Implementation done
- ✅ Stage 5c: Refactoring completed
- ✅ Stage 6: Build successful
- ✅ Stage 7: Security review passed
- ✅ Stage 8: Code review passed
- ✅ Stage 9: Design review (Status: Pass or Skipped)
- ✅ Stage 10: Coverage verified (Status: Pass, >= 80%)
- ✅ Stage 11: Tests passed
- ✅ Stage 12: User approved

**If any artifact is missing or status is Failed → return to Stage 3.**

---

## For Claude Code / Codex Implementers

When executing `implement issue` command:

1. **Read this file FIRST**
2. **Read `.ai/rules/no-stage-skipping-even-for-simple-tasks.md` SECOND**
3. Execute stages in strict order: 1 → 2 → 3 → 3.5 → 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13
4. Do NOT skip stages 4, 7, 8, 10, 12
5. For Stage 12, use AskUserQuestion tool to get approval before committing
6. Create artifacts for each stage in `.ai/artifacts/run/`
7. If any stage fails, run debug-rca agent and return to Stage 3

