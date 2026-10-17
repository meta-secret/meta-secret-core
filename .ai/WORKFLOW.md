# Automated Workflow Orchestration — 14 Stages (including the conditional UI E2E gate)

Coverage follows [the advisory policy](rules/coverage-policy.md); percentage-only failures in older examples are not release gates.

For the approved registration migration, apply the [UI migration profile](rules/ui-migration-profile.md) before command/stage examples below. It defines relevant client builds, advisory percentage coverage, mandatory visual/motion review and action-specific authorization; all existing stage artifacts remain required.

Single source of truth for `implement issue <id>` / `implement issue "<text>"` for meta-secret-core Rust and Vue Web changes.

---

## ⚠️ CRITICAL: Non-Optional Stages

**These stages MUST ALWAYS be executed and are NOT optional:**

1. **Stage 7 (Security Review)** — Mandatory security review; crypto/unsafe checks when affected
2. **Stage 8 (Code Review)** — Mandatory constraints validation + coverage report (80% advisory)
3. **Stage 10 (Coverage Verification)** — Mandatory applicable coverage reporting; Rust tooling only for Rust changes; 80% advisory
4. **Stage 12 (User Approval)** — Mandatory user approval before PR

Stage 9 (Design Review) may be skipped only if neither architecture nor client-visible UI/motion changes; mark status as "Skipped".

The conditional **Stage 11.5 (Final UI E2E)** is mandatory for applicable
client-visible, synchronization/recovery, FFI-consumed, or explicitly requested
UI behavior. Otherwise it must be marked `Skipped` with a concrete reason.

**If any of these stages are missing from execution, the workflow is INCOMPLETE and INVALID.**

---

## Command Contract

- **Trigger:** `implement issue <id-or-text>`
- **Optional resume:** `implement issue <id-or-text> --from stage-<n>`
- **Artifacts directory:** `.ai/artifacts/run/`
- **Artifact naming:** `MS-<run-id>-<stage-number>-<stage-name>[ -retry-N ].md`
- **Retry budget:** `2` full fix loops

## Root orchestration mode

When started by the MetaSecret root via `get_issue`, consume its normalized
task bundle: Issue metadata when present, otherwise approved local brief path
and reference commit, title/body, approval evidence, root run ID, scope/areas,
profile and dependency handoffs. Do not fetch a fabricated Issue for local input.

This repository retains all existing stage artifacts and applicable gates.
For `core+compose`, defer application release commits until combined validation
and the root final UI E2E gate pass. Domain/FFI-dependent Compose work consumes
Core's successful contract handoff; UI-only Web and Compose work independently
consume validated shared design exports. Web-only scope in Core repository
uses the UI migration profile rather than unrelated Rust build/coverage checks.
Explicit session authorization is recorded per release action; commit permission
does not imply push/PR permission. Root-only resources do not start this child
application workflow.

**`<run-id>` rules:**
- Numeric issue input: use issue number (`123`)
- Free-text/local brief input: use UTC timestamp (`YYYYMMDDHHmmss`)

---

## Required Stage Logs

**Every stage MUST print:**
- Start: `Start stage <n>: <name>`
- End: `Stage <n>: <name> completed`

Example:
- `📋 Start stage 1: Issue Coordinator`
- `✅ Stage 1: Issue Coordinator completed`

---

## 14-Stage Pipeline (with TDD + Security)

```
┌─ Intake & Analysis
│  1. Issue Coordinator (understand problem)
│  2. Requirements Clarifier (grill user with questions)
│
├─ Planning & Validation
│  3. Feature Planner (create implementation plan)
│  3.5. Constraint Validator (GATE - validate against CONSTRAINTS.md)
│
├─ Implementation (TDD)
│  5a. TDD Test Author (write failing tests)
│  5b. TDD Implementer (red-green-refactor cycles)
│  5c. TDD Refactorer (major refactoring)
│
├─ Build & Validation
│  6. Build (cargo build)
│  7. Security Review ⭐ (crypto audit + unsafe code check)
│  8. Code Review (architecture + constraints + coverage check)
│  9. Design Review (client UI/motion; diagrams when architecture changes)
│
├─ Quality Gates
│  10. Coverage Verification (coverage report; 80% advisory)
│  11. Test Run (cargo test)
│  11.5. Final UI E2E (conditional, before release)
│
└─ Release
   12. User Approval (STOP and ASK USER)
   13. release-manager (branch + commit + PR)
```

---

## Stage Specs

### Stage 1: Issue Intake + Optional Architecture Context

**Agent:** `github-issue-coordinator`  
**Template:** `.ai/artifacts/issue-analysis-template.md`  
**Output:** `.ai/artifacts/run/MS-<run-id>-001-understanding.md`

**Required behavior:**
- Read issue (or free-text task)
- Understand scope and impact
- If issue mentions architecture/protocol/crypto: extract key concepts
- Produce output following template
- Include: `Architecture Changed: YES/NO`

---

### Stage 2: Grill Me (Clarification & Deep Dive)

**Agent:** `requirements-clarifier`  
**Template:** `.ai/artifacts/clarification-template.md`  
**Input:**
- Stage 1 artifact

**Output:** `.ai/artifacts/run/MS-<run-id>-002-clarification.md`

**Duration (adaptive - stop when shared understanding reached):**
- Simple tasks (bug fix, style update): 5-10 minutes
- Medium tasks (feature addition): 15-25 minutes
- Complex tasks (protocol, crypto, resharing): 30-45 minutes

**Required behavior:**
- Use "Grill Me" methodology: walk decision tree, resolve dependencies
- Ask clarifying questions about unclear/risky areas ONLY (not all possible questions)
- Validate crypto assumptions (if touching cryptography)
- Validate FFI boundary changes (if touching UniFFI exports)
- Provide recommendations for each question
- Reuse explicit session approvals; ask only unresolved decisions
- Document all clarifications in artifact

---

### Stage 3: Planning

**Agent:** `feature-planner`  
**Template:** `.ai/artifacts/implementation-plan-template.md`  
**Input:**
- Stage 1 artifact
- Stage 2 artifact (clarifications)
- If retry: failed artifact from Stage 6/7/8/11

**Output:** `.ai/artifacts/run/MS-<run-id>-003-planning.md`

**Required behavior:**
- Create implementation plan aligned with architecture/security/style
- Incorporate all clarifications from Stage 2
- If architecture changed: document design decisions
- If crypto touched: document SSS/key management changes
- If FFI changed: document impact on mobile (meta-secret-compose)
- Complete the **Documentation Impact** section in the plan. Mark it
  `Required` and list files/sections when behavior, terminology, constraints,
  architecture, protocol/API contracts, or workflows change. Mark it `Not
  required` only with a concrete reason.
- If retry: add explicit fix plan derived from failure artifact
- Include test strategy

---

### Stage 3.5: Constraint Validation (MANDATORY GATE)

**Agent:** `constraint-validator`  
**Input:**
- Stage 3 (Planning artifact)

**Output:** `.ai/artifacts/run/MS-<run-id>-0035-constraints.md`

**Required behavior:**
- Validate plan against `.ai/CONSTRAINTS.md`
- Check K-of-N sharing rules (if device/vault logic changes)
- Check approval requirements (if JOIN/RESTORE/REMOVE changes)
- Check server zero-knowledge principle (if server logic changes)
- Check FFI stability (if UniFFI boundary changes)
- Check crypto assumptions (if SSS/key derivation changes)

**Status:** Pass / Fail

**If Fail:**
- Document which constraint(s) violated
- Return to Stage 3 (Planning) with constraint violation details
- Planner must re-architect to satisfy constraints

---

### Stage 5a: TDD Test Author

**Agent:** `tdd-test-author`  
**Input:**
- Stage 3 (Planning)

**Output:** `.ai/artifacts/run/MS-<run-id>-005a-tests.md`

**Required behavior:**
- Write failing tests covering all requirements from plan
- Unit tests for individual functions
- Integration tests for workflows (if applicable)
- Property tests for crypto (if touching SSS/key derivation)
- Tests must fail before implementation
- Include test strategy in artifact

---

### Stage 5b: TDD Implementer (Red-Green-Refactor)

**Agent:** `tdd-implementer`  
**Input:**
- Stage 5a tests
- Stage 3 plan

**Output:** `.ai/artifacts/run/MS-<run-id>-005b-implementation.md`

**Required behavior:**
- Execute red-green-refactor cycles
- RED: write minimal code to make 1 test pass
- GREEN: verify test passes
- After 3-5 cycles: refactor (Stage 5c)
- Respect Rust idioms (ownership, borrowing, error handling)
- Respect crate boundaries and FFI stability

---

### Stage 5c: TDD Refactorer

**Agent:** `tdd-refactorer`  
**Input:**
- Stage 5b implementation
- Stage 5a tests

**Output:** `.ai/artifacts/run/MS-<run-id>-005c-refactored.md`

**Required behavior:**
- Major refactoring after 3-5 red-green cycles
- Remove duplication
- Improve naming and structure
- All tests must still pass
- Verify no unsafe code added unless justified

---

### Stage 6: Build

**Command:** `cargo build --release`  
**Template:** `.ai/artifacts/build-report-template.md`  
**Output:** `.ai/artifacts/run/MS-<run-id>-006-build.md`

**Required behavior:**
- Compile changed targets; use the UI profile client build for Web-only changes
- No tests executed
- Timeout: 10 minutes max
- Fail if compilation errors
- Report all warnings (treat as information)

**Status:** Success / Failed

---

### Stage 7: Security Review ⭐ NEW!

**Agent:** `security-reviewer`  
**Input:**
- Stage 6 (compiled code)
- Stage 5c (implementation)

**Output:** `.ai/artifacts/run/MS-<run-id>-007-security-review.md`

**Required behavior:**
- If crypto code touched: audit SSS/key derivation algorithm correctness
- If device logic touched: verify resharing protocol assumptions
- If FFI changed: check for unintended exposure (no secrets over FFI)
- Run: `cargo clippy --all-targets -- -D warnings`
- Audit: all `unsafe` blocks (justify each one)
- Check: no plaintext logging of secrets/keys
- Verify: E2E encryption assumptions still hold

**Status:** Pass / Fail

**If Fail:**
- Document security concerns
- Return to Stage 3 (Planning) with security notes
- May require design changes, not just code fixes

---

### Stage 8: Code Review

**Agent:** `code-reviewer`  
**Input:**
- Stage 6 (build report)
- Stage 5c (implementation)
- Stage 3 (plan)

**Output:** `.ai/artifacts/run/MS-<run-id>-008-code-review.md`

**Required behavior:**
- Architecture compliance: does code follow ARCHITECTURE.md?
- Constraint re-validation: ensure plan compliance maintained
- Coverage check: report measured coverage; 80% advisory (detailed in Stage 10)
- Style and best practices: Rust idioms, crate organization
- FFI impact: if boundary changes, note mobile compatibility
- Performance: any crypto operations performant?
- Documentation Impact: verify every item marked `Required` in the plan is
  updated, or record the exact blocker and fail the review. If marked `Not
  required`, verify that the reason is still valid.

**Status:** Pass / Fail

---

### Stage 9: Design Review (CONDITIONAL)

**Condition:** Run for client-visible UI/motion changes or an architecture change in the Stage 3 plan

**Agent:** `design-reviewer`  
**Template:** `.ai/artifacts/design-review-report-template.md`  
**Output:** `.ai/artifacts/run/MS-<run-id>-009-design-review.md`

**Required behavior (choose based on change type):**

**For Web/Frontend changes:**
- Compare UI and motion against approved HTML prototype, screenshots and specification; include Figma when supplied
- Verify design constraints met

**For Core/Backend changes:**
- Create protocol diagram (if communication changed)
- Create algorithm diagram (if crypto/SSS changed)
- Create state machine diagram (if device states changed)
- Verify all diagrams match implementation

**If no architecture change and no client-visible UI/motion change:**
- Status: Skipped
- Reason: "No architecture or client-visible UI/motion changes in Stage 3 plan"

**Status:** Success / Failed / Skipped

---

### Stage 10: Coverage Verification (CRITICAL)

**Command:** configured coverage reporting for changed targets; for applicable Rust runs, `cargo tarpaulin --out Html --timeout 300`
**Template:** `.ai/artifacts/coverage-report-template.md`  
**Output:** `.ai/artifacts/run/MS-<run-id>-010-coverage.md`

**Required behavior:**
- Run applicable configured coverage reporting; use Rust tooling only for Rust changes
- Overall goal: 80% advisory
- Crypto modules preferred: >= 95%
- Report uncovered lines (which lines missed coverage)
- Report coverage < 80% as advisory; fail on actual test/report validation errors

**Status:** Pass / Fail (not Success/Failed, use Pass/Fail for coverage)

**If Fail:**
- Document which lines/modules lack coverage
- Return to Stage 3 (Planning) to add tests
- Must re-run Stages 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10

---

### Stage 11: Test Run (Final Validation)

**Command:** `cargo test --all`  
**Template:** `.ai/artifacts/test-report-template.md`  
**Output:** `.ai/artifacts/run/MS-<run-id>-011-test-run.md`

**Required behavior:**
- Execute relevant unit/integration suites for the changed targets; preserve required configured CI checks
- Parallel execution (safe for Rust)
- Report pass/fail for each test
- Capture failing test details
- All tests must pass

**Status:** Success / Failed

**If Fail:**
- Document which test failed and why
- Return to Stage 3 (Planning)
- May require design changes if tests reveal flaws

### Stage 11.5: Final UI E2E (CONDITIONAL RELEASE GATE)

**Agents:** root `ui-e2e-test-author`, then root `ui-e2e-test-runner`
**Skill:** root `.ai/skills/ui-e2e-testing/SKILL.md`
**Contract:** root `.ai/rules/ui-e2e-test-contract.md`
**Template:** root `.ai/artifacts/ui-e2e-report-template.md`
**Artifact:** `.ai/artifacts/run/MS-<run-id>-0115-ui-e2e.md`

Run only after Stages 6–11 and the documentation check have passed. The gate
is required when the change affects client-visible behavior, client-consumed
FFI/API behavior, synchronization/recovery, or explicitly requested UI E2E.
Otherwise record `Status: Skipped` and the concrete reason.

The author prepares the role matrix and scenario using the existing
`e2eTest` harness. The runner executes Web visibly, boots/unlocks iOS and
Android, uses CLI only when the scenario requires it, and runs blocks
sequentially. Every relevant platform occupies every relevant role and every
variant runs three cycles by default; a lower count requires a documented
resource reason. Only marker/state/deadline waits are allowed—no arbitrary
sleep, fixed delay, or `slowMo`.

The report must include exact commands, block/cycle/role assignments, marker
timeline, Recover/Show decisions from current Core state, receiver badge/alert
assertions, screenshots, browser/server/device logs, cleanup, and any
deviation. A failure blocks Stage 12. Scenario-only failures may return to the
author; evidence of a production defect returns to planning and follows the
normal approval rule. In root `core+compose` mode this stage is deferred to the
root final gate after Compose validation.

---

### Stage 12: User Approval

**Agent:** `release-manager`  
**Output:** `.ai/artifacts/run/MS-<run-id>-012-approval.md`

**Required behavior:**
- Record explicit session authorization for each release action.
- Reuse existing authorization for commits; ask only for a still unauthorized action after presenting a concrete result.
- Commit permission does not authorize push, PR, merge or deployment.
- If the user declines an action, stop that action and retain the reviewed work.

**Status:** Success (user approved) / Cancelled (user declined)

---

### Stage 13: Branch + Commit + PR

**Agent:** `release-manager`  
**Output:** `.ai/artifacts/run/MS-<run-id>-013-pr.md`

**Required behavior:**
- Reuse the current task branch outside main, or create an appropriate feature branch without discarding existing work.
- Commit only reviewed task changes when explicitly authorized.
- Write meaningful commit message
- Only when separately authorized, push and create a pull request with:
  - Link to the original Issue when present; otherwise identify the approved local task without inventing a public URL
  - Summary of changes
  - Coverage report link
  - Security review notes (if applicable)
- Ensure CI passes

**Status:** Success / Failed

---

## Retry Rules

**Retry trigger:**
- Build failed (Stage 6)
- Security review failed (Stage 7)
- Code review failed (Stage 8)
- Coverage failed (Stage 10)
- Test run failed (Stage 11)
- Final UI E2E failed (Stage 11.5)

**Retry path:**
- Return to Stage 3 (Planning) with failed artifact as input
- Re-run stages from Stage 5a onward
- Max retries: 2 full loops

---

## Failure Markers

- `Status: FAILED`
- `Return to Planning: YES`
- `**FAIL**`
- `FAIL`

---

## Artifact System

### Every Stage Creates an Artifact

Each stage writes output to `.ai/artifacts/run/` following naming convention:

```
MS-<run-id>-<stage-number>-<stage-name>[ -retry-N ].md
```

**Example:** `MS-42-010-coverage.md` or `MS-42-010-coverage -retry-1.md`

### Status Field (REQUIRED)

Every artifact must have **Status** at the top:

```markdown
**Status:** Success | Failed | Skipped
```

Validation stages use:
```markdown
**Status:** Pass | Fail | Skipped
```

---

## Validation Checklist Before Stage 13

Before proceeding to Stage 13 (PR), verify:

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
- ✅ Stage 9: Design review (Status: Success or Skipped)
- ✅ Stage 10: Coverage report validated (Status: Pass; percentage advisory)
- ✅ Stage 11: Tests passed
- ✅ Stage 11.5: Final UI E2E passed, or was explicitly skipped with a reason
- ✅ Stage 12: User approved

If any artifact is missing or status is Failed → return to Stage 3.

---

**Next:** See `.ai/ORCHESTRATOR.md` for command routing.

## Workflow dependency preflight

Before starting a workflow run `python3 .ai/scripts/check-workflow-dependencies.py`. Missing mandatory files block execution. Root UI E2E dependencies belong to the sibling workspace root and must be checked there before the final gate.
