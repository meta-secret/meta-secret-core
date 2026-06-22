# Command — Implement Issue

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

Execute complete 14-stage automated workflow for meta-secret-core Rust backend.

**⚠️ CRITICAL:** All 14 stages are MANDATORY. Do NOT skip any stages:
- **Stage 6 (Security Review)** is CRITICAL — must check crypto correctness + FFI changes
- **Stage 7 (Code Review)** is CRITICAL — must check constraints + coverage
- **Stage 8 (Design Review)** can be skipped only if no architecture changes
- **Stage 9 (Coverage Verification)** is CRITICAL — must verify >= 80%
- **Stage 11 (User Approval)** must ASK USER before creating PR

Even if task looks "trivial" or "1-line change", execute ALL stages.

## Flow

1. **github-issue-coordinator** — Analyze Rust/crypto issue (with optional architecture diagrams)
2. **requirements-clarifier** — Deep dive clarification (Grill Me - validate design implications)
3. **feature-planner** — Create implementation plan (aligned with CONSTRAINTS.md)
4. **constraint-validator** — Validate plan against CONSTRAINTS.md (MANDATORY GATE)
5. **TDD Implementation** (Test-Driven Development):
   - 5a. **tdd-test-author** — Write failing tests (unit + integration + property tests if crypto)
   - 5b. **tdd-implementer** — Red-Green-Refactor cycles (minimal code → pass tests)
   - 5c. **tdd-refactorer** — Major refactoring after 3-5 cycles
6. **Build** — Compile code with `cargo build` (no tests, max 10 minutes)
7. **Security Review** — Crypto correctness + unsafe code audit + FFI boundary impact ⭐ NEW
8. **Code Review** — Architecture, style, constraints, 80% coverage check
9. **Design Review** — If architecture/protocol changed: create diagrams (else Skipped)
10. **Coverage Verification** — Run `cargo tarpaulin`, verify >= 80% coverage
11. **Test Run** — Execute `cargo test` (all tests pass)
12. **User Approval** — **STOP and ASK USER:** "Should we proceed to Stage 12 (Branch + Commit + PR)?"
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
- **Stage 10:** `MS-<run-id>-010-coverage.md` — Coverage verification (Pass/Fail, >= 80%)
- **Stage 11:** `MS-<run-id>-011-test-run.md` — Test execution results (Pass/Fail)
- **Stage 12:** `MS-<run-id>-012-pr.md` — PR details (Success/Failed)

Each artifact includes **Status: Success / Failed / Skipped**.

See `.ai/rules/artifact-writing-guide.md` for artifact specification.

---

## Build & Test Commands

### Stage 6: Build
```bash
cargo build --release
```
- Compile entire workspace
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
cargo tarpaulin --out Html --timeout 300 --fail-under 80
```
- Generate HTML report in `target/tarpaulin/`
- Minimum threshold: 80%
- Crypto modules preferred: >= 95%
- Fail if coverage < 80%

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
| **Design Review** | Figma (optional) | Figma (web) or diagrams (core) |
| **Language** | Kotlin (mobile) | Rust (backend) |

---

## Critical Rules

1. **All stages execute in strict order** — no skipping based on complexity
2. **Security Review is mandatory** — crypto code must be audited
3. **Coverage >= 80%** — non-negotiable threshold
4. **FFI boundary changes** — must note impact on mobile (meta-secret-compose)
5. **Constraint re-check** — Stage 8 validates against `.ai/CONSTRAINTS.md`
6. **User approval** — Stage 12 asks before PR creation

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
6. **For Stage 12 (User Approval):** Use `AskUserQuestion` tool — do NOT auto-commit
7. **If failure:** Run debug-rca agent, return to Stage 3

---

**Next:** See `.ai/WORKFLOW.md` for complete 14-stage specification with details on each stage.
