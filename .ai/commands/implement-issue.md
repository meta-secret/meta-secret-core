# Command — Implement Issue

Coverage follows [the advisory policy](../rules/coverage-policy.md); percentage-only failures in older examples are not release gates.

For the approved registration migration, apply the [UI migration profile](../rules/ui-migration-profile.md) before command/stage examples below. It defines relevant client builds, advisory percentage coverage, mandatory visual/motion review and action-specific authorization; all existing stage artifacts remain required.

## Trigger

```
implement issue <payload>
implement issue <payload> --from stage-<n>
```

Where `<payload>`:
- Issue number: `#42`
- Free-text description: `"implement device resharing protocol"`
- Issue URL: `https://github.com/org/repo/issues/42`

## Purpose

Execute complete 14-stage automated workflow for meta-secret-core Rust and Vue Web changes,
including the conditional final UI E2E release gate.

**⚠️ CRITICAL:** All 14 stages are MANDATORY. Do NOT skip any stages:
- **Stage 7 (Security Review)** is CRITICAL — must check crypto correctness + FFI changes
- **Stage 8 (Code Review)** is CRITICAL — must check constraints + coverage
- **Stage 9 (Design Review)** can be skipped only if neither architecture nor client-visible UI/motion changes
- **Stage 10 (Coverage Verification)** is CRITICAL — must report measured coverage; 80% advisory
- **Stage 12 (User Approval)** records existing explicit authorization; asks only for a still unauthorized release action

Even if task looks "trivial" or "1-line change", execute ALL stages.

## Flow

1. **github-issue-coordinator** — Analyze Rust/crypto issue (with optional architecture diagrams)
2. **requirements-clarifier** — Deep dive clarification (Grill Me - validate design implications)
3. **feature-planner** — Create implementation plan (aligned with CONSTRAINTS.md)
3.5. **constraint-validator** — Validate plan against CONSTRAINTS.md (MANDATORY GATE)
5. **TDD Implementation** (Test-Driven Development):
   - 5a. **tdd-test-author** — Write failing tests (unit + integration + property tests if crypto)
   - 5b. **tdd-implementer** — Red-Green-Refactor cycles (minimal code → pass tests)
   - 5c. **tdd-refactorer** — Major refactoring after 3-5 cycles
6. **Build** — Compile code with `cargo build` (no tests, max 10 minutes)
7. **Security Review** — Crypto correctness + unsafe code audit + FFI boundary impact ⭐ NEW
8. **Code Review** — Architecture, style, constraints, Documentation Impact, coverage report (80% advisory)
9. **Design Review** — Review approved visuals/motion for client-visible UI; diagrams for changed architecture/protocol; otherwise record non-applicability
10. **Coverage Verification** — Run `cargo tarpaulin`, report coverage; 80% advisory
11. **Test Run** — Execute `cargo test` (all tests pass)
11.5. **Final UI E2E** — Run the visible cross-platform gate when applicable
12. **User Approval** — Record authorization per release action; ask only if missing
13. **release-manager** — Create branch, commit, pull request

See `.ai/WORKFLOW.md` for complete 14-stage specification.

## Expected Input

- GitHub issue number (e.g., `#42`)
- GitHub issue URL
- Free-text task description

## Output

- Pull Request with implementation, tests, and documentation
- All artifacts stored in `.ai/artifacts/run/`

---

## Artifacts Generated

Each stage creates an artifact in `.ai/artifacts/run/`:

- **Stage 1:** `MS-<run-id>-001-understanding.md` — Issue analysis
- **Stage 2:** `MS-<run-id>-002-clarification.md` — Clarifications & decisions
- **Stage 3:** `MS-<run-id>-003-planning.md` — Implementation plan
- **Stage 3.5:** `MS-<run-id>-0035-constraints.md` — Constraint validation (Pass/Fail)
- **Stage 5a:** `MS-<run-id>-005a-tests.md` — Failing test cases
- **Stage 5b:** `MS-<run-id>-005b-implementation.md` — Implementation (red-green cycles)
- **Stage 5c:** `MS-<run-id>-005c-refactored.md` — Refactored code
- **Stage 6:** `MS-<run-id>-006-build.md` — Build report (Success/Failed)
- **Stage 7:** `MS-<run-id>-007-security-review.md` — Security review findings (Pass/Fail)
- **Stage 8:** `MS-<run-id>-008-code-review.md` — Code review findings (Pass/Fail)
- **Stage 9:** `MS-<run-id>-009-design-review.md` — Design review or "Skipped"
- **Stage 10:** `MS-<run-id>-010-coverage.md` — Coverage verification (Pass/Fail; percentage advisory)
- **Stage 11:** `MS-<run-id>-011-test-run.md` — Test execution results (Pass/Fail)
- **Stage 11.5:** `MS-<run-id>-0115-ui-e2e.md` — Final visible UI E2E gate
- **Stage 12:** `MS-<run-id>-012-pr.md` — PR details (Success/Failed)

Each artifact includes **Status: Success / Failed / Skipped**.

See `.ai/rules/artifact-writing-guide.md` for artifact specification.

---

## Build & Test Commands

The commands below apply to Rust changes. For Web-only work, use the UI
migration profile and `.ai/skills/web-guardian/SKILL.md`: build/typecheck the
changed Web client and run relevant configured tests/reporting. Do not invoke
Rust coverage or rebuild unchanged native libraries for Vue/CSS-only edits.

### Stage 6: Build
```bash
cargo build --release
```
- Compile changed targets; use the UI profile client build for Web-only changes
- No tests executed
- Timeout: 10 minutes max
- Fail if compilation errors

### Stage 7: Security Review
```bash
# Manual review of:
cargo clippy --all-targets -- -D warnings
# Check for unsafe code:
grep -n "unsafe" src/ crates/*/src/
# Manual crypto audit (if touched cryptography modules)
```

### Stage 10: Coverage Verification
```bash
cargo tarpaulin --out Html --timeout 300
```
- Generate HTML report in `target/tarpaulin/`
- Overall goal: 80% advisory
- Crypto modules preferred: >= 95%
- Report coverage < 80% as advisory; fail on actual test/report validation errors

### Stage 11: Test Run
```bash
cargo test --all
```
- Run all unit + integration tests
- Parallel execution (safe for Rust)
- Fail if any test fails

---

## Key Differences from Compose

| Aspect | Compose | Core |
|---|---|---|
| **Build** | `./gradlew build` | `cargo build` |
| **Test** | `./gradlew test` | `cargo test` |
| **Coverage** | `./gradlew koverReport` | `cargo tarpaulin` |
| **Security** | Not explicit | Stage 7: explicit crypto review |
| **Design Review** | Approved HTML/screenshots/motion; Figma optional | Approved Web visuals/motion; relevant Core diagrams |
| **Language** | Kotlin (mobile) | Rust (backend) |

---

## Critical Rules

1. **All stages execute in strict order** — no skipping based on complexity
2. **Security Review is mandatory** — crypto code must be audited
3. **Measured coverage** — overall 80% is advisory; required behavior tests must pass
4. **FFI boundary changes** — must note impact on mobile (meta-secret-compose)
5. **Constraint re-check** — Stage 8 validates against `.ai/CONSTRAINTS.md`
6. **Documentation Impact** — Stage 3 records Required/Not required with a reason; final review verifies the decision
7. **Final UI E2E** — Stage 11.5 runs after all regular checks and before approval/commit when applicable
8. **User approval** — Stage 12 records authorization for each release action; existing commit permission does not permit PR/push

---

## Failure Recovery

If any stage fails:
1. Identify root cause
2. Document in artifact with **Status: Failed**
3. Return to Stage 3 (Planning) with failed artifact as input
4. Create fix plan based on failure
5. Re-execute stages 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10 → 11 → 12
6. Max retries: 2 full loops

---

## For Implementers (Agents)

When executing `implement issue`:

1. **Read first:** `.ai/rules/implement-issue-execution-checklist.md` + `.ai/rules/no-stage-skipping-even-for-simple-tasks.md`
2. **Execute stages in strict order:** 1 → 2 → 3 → 3.5 → 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10 → 11 → 12
3. **Do NOT skip stages** — even if task looks simple
4. **Create artifacts for ALL stages** — check that all files exist in `.ai/artifacts/run/`
5. **For Stage 7 (Security):**
   - Run `cargo clippy`
   - Check for `unsafe` blocks
   - If crypto code changed: audit algorithm correctness
   - If FFI changed: note impact on mobile
6. **For Stage 12 (User Approval):** Record existing explicit session permission; ask only for a still unauthorized release action
7. **If failure:** Run debug-rca agent, return to Stage 3

---

**Next:** See `.ai/WORKFLOW.md` for complete 14-stage specification with details on each stage.
