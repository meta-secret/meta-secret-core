# Rule: No Stage Skipping — Even for Simple Tasks (meta-secret-core)

**CRITICAL ENFORCEMENT RULE**

---

## ⚠️ MANDATORY: Execute ALL 14 stages for EVERY task

**Even if:**
- Task requires only 1 line of code changes
- Task is "trivial" or "simple"
- Task is a bug fix with obvious solution
- Task only touches one file
- Task seems to require no testing

**Execute ALL 14 stages in strict order:**

```
Stage 1 → Stage 2 → Stage 3 → Stage 3.5 → Stage 5a → Stage 5b → Stage 5c → 
Stage 6 → Stage 7 → Stage 8 → Stage 9 → Stage 10 → Stage 11 → Stage 12 → Stage 13
```

---

## ❌ DO NOT Skip These Stages

### Stage 2 (Requirements Clarifier)
- **MUST execute** even for "obvious" tasks
- "Obvious" to you might be wrong
- Clarification can catch hidden assumptions
- Always grill user with questions

### Stage 3 (Feature Planner)
- **MUST execute** even for 1-line fixes
- Plan ensures architectural alignment
- Plan documents the decision
- Shortcut only leads to bugs later

### Stage 3.5 (Constraint Validator)
- **MUST execute** even for small changes
- Constraints are not "optional suggestions"
- One-line change can violate architectural rules
- Gate must not be skipped

### Stage 5a (TDD Test Author)
- **MUST execute** even if "change is obvious"
- Write failing tests FIRST
- Tests define expected behavior
- No exceptions

### Stage 5b (TDD Implementer)
- **MUST execute** even for simple fixes
- Red-Green-Refactor is mandatory
- Not "optional for small changes"
- 1-line change still needs RED → GREEN cycle

### Stage 5c (TDD Refactorer)
- **MUST execute** after 3-5 cycles
- Even 1-line changes might have refactoring opportunities
- Clean code is not "optional for simple tasks"

### Stage 6 (Build)
- **MUST execute** always
- Command: `cargo build --release`
- Compilation is non-negotiable
- Even "obvious" changes can break builds

### Stage 7 (Security Review)
- **MANDATORY** even for trivial changes
- Crypto audit if crypto touched
- Check: `cargo clippy --all-targets -- -D warnings`
- FFI boundary review if needed

### Stage 8 (Code Review)
- **MANDATORY** even for trivial changes
- Coverage check is CRITICAL
- 1-line change must still meet 80% coverage requirement
- Constraints re-check is not "optional"

### Stage 9 (Design Review)
- **MUST execute** if architecture changed
- Or explicitly mark "Skipped" if no architecture changes
- Not: "Skip because change is simple"

### Stage 10 (Coverage Verification)
- **MANDATORY** even for bug fixes
- Command: `cargo tarpaulin --out Html --timeout 300 --fail-under 80`
- Coverage >= 80% is non-negotiable
- No exceptions for "trivial" changes

### Stage 11 (Test Run)
- **MUST execute** always
- Command: `cargo test --all`
- All tests must pass
- Not "optional for 1-line changes"

### Stage 12 (User Approval)
- **MANDATORY** gate before PR
- Must ask user: "Should we proceed to Stage 13?"
- Never auto-commit or auto-create PR
- User approval required 100% of the time

### Stage 13 (Release Manager)
- **MUST execute** after approval
- Create branch, commit, and PR
- Cannot be skipped once approved

---

## Why All Stages for All Tasks?

1. **Quality assurance** — Stages catch bugs early
2. **Consistency** — Same process prevents skips
3. **Architectural compliance** — Constraints apply to all changes
4. **Test coverage** — All code needs tests
5. **Code review** — All code needs review
6. **Audit trail** — All stages create artifacts

---

## Examples: Tasks That Look "Simple" But Need All Stages

### Example 1: Fix one line in EmailConfirmationScreen.kt
```kotlin
// Before
navigator?.popUntilRoot()

// After
navigator?.popUntilRoot()
navigator?.push(SignInScreen())
```

**This STILL needs:**
- ✅ Stage 2: Grill user — "Is this the only change needed?"
- ✅ Stage 3: Plan — "How does this fit architecture?"
- ✅ Stage 3.5: Validate — "Violates any constraints?"
- ✅ Stage 5a: Test — "What tests cover this?"
- ✅ Stage 5b: Implement — "Red-Green cycle"
- ✅ Stage 5c: Refactor — "Better way to do this?"
- ✅ Stage 6: Build — "Compiles?"
- ✅ Stage 7: Security — "Unsafe code OK? FFI safe?"
- ✅ Stage 8: Review — "Coverage OK? Constraints OK?"
- ✅ Stage 9: Design — "Architecture still valid?"
- ✅ Stage 10: Coverage — ">= 80%?"
- ✅ Stage 11: Tests — "All pass?"
- ✅ Stage 12: Approval — "User approves?"
- ✅ Stage 13: Commit — "Create PR"

### Example 2: Button color change
```kotlin
// Before
Button(backgroundColor = Color.Blue)

// After
Button(backgroundColor = Color.Red)
```

**This STILL needs all 14 stages** because:
- Color might violate design constraints
- Need to verify design
- Need test coverage
- Need code review
- Need user approval

### Example 3: Typo fix
```rust
// Before
eprintln!("Emai confirmation");

// After
eprintln!("Email confirmation");
```

**This STILL needs all 14 stages** (though will be quick):
- Stage 2: "Is this the only typo?"
- Stage 3: "Any other places this typo appears?"
- Stage 5a: Quick test for the affected code path
- Stage 5b: Implement fix
- etc.

---

## Red Flags: You're About to Skip Stages If You Think...

- ❌ "This is so simple, I'll skip Stage 2"
- ❌ "1 line of code, don't need Stage 3"
- ❌ "It's obvious, can skip Stage 4a (tests)"
- ❌ "No point in Stage 6 (review) for this"
- ❌ "Coverage check is overkill for small changes"
- ❌ "Can auto-commit without Stage 10 (approval)"

**ALL of these are WRONG. Execute ALL stages.**

---

## How to Handle Simple vs. Complex Tasks

**Simple task:** All 14 stages execute quickly
- Stage 2: 2 minutes (quick clarification)
- Stage 3: 5 minutes (simple plan)
- Stage 3.5: 2 minutes (constraint check)
- Stage 5a-5c: 10 minutes (trivial tests + refactoring)
- Stage 6: 5 minutes (cargo build)
- Stage 7: 3 minutes (security review)
- Stage 8: 5 minutes (code review)
- Stage 9: 2 minutes (design review or skip)
- Stage 10: 5 minutes (coverage check)
- Stage 11: 5 minutes (cargo test)
- Stage 12: 2 minutes (ask user approval)
- Stage 13: 5 minutes (create PR)

**Total: ~50 minutes for "simple" task through all stages**

**Complex task:** All 14 stages take longer
- Stage 2: 30 minutes (deep clarification)
- Stage 3: 45 minutes (complex plan)
- Stage 3.5: 10 minutes (detailed constraint validation)
- Stage 5a-5c: 120 minutes (extensive TDD cycles)
- Stage 6: 10 minutes (cargo build)
- Stage 7: 30 minutes (detailed security review)
- Stage 8: 30 minutes (thorough code review)
- Stage 9: 45 minutes (design review with diagrams)
- Stage 10: 10 minutes (coverage analysis)
- Stage 11: 10 minutes (cargo test)
- Stage 12: 5 minutes (ask user approval)
- Stage 13: 10 minutes (create PR)

**Total: ~5+ hours for complex task**

**Both execute ALL stages. Difference is duration, not skipping.**

---

## Enforcement: Check Artifacts

After any `implement issue <task>` execution, verify:

```
.ai/artifacts/run/MS-<id>-001-*.md       ← Stage 1 (Issue Coordinator)
.ai/artifacts/run/MS-<id>-002-*.md       ← Stage 2 (Requirements Clarifier) 
.ai/artifacts/run/MS-<id>-003-*.md       ← Stage 3 (Feature Planner)
.ai/artifacts/run/MS-<id>-0035-*.md      ← Stage 3.5 (Constraint Validator) ⚠️ CRITICAL
.ai/artifacts/run/MS-<id>-005a-*.md      ← Stage 5a (TDD Test Author)
.ai/artifacts/run/MS-<id>-005b-*.md      ← Stage 5b (TDD Implementer)
.ai/artifacts/run/MS-<id>-005c-*.md      ← Stage 5c (TDD Refactorer)
.ai/artifacts/run/MS-<id>-006-*.md       ← Stage 6 (Build)
.ai/artifacts/run/MS-<id>-007-*.md       ← Stage 7 (Security Review) ⚠️ CRITICAL
.ai/artifacts/run/MS-<id>-008-*.md       ← Stage 8 (Code Review) ⚠️ CRITICAL
.ai/artifacts/run/MS-<id>-009-*.md       ← Stage 9 (Design Review) or Skipped
.ai/artifacts/run/MS-<id>-010-*.md       ← Stage 10 (Coverage Verification) ⚠️ CRITICAL
.ai/artifacts/run/MS-<id>-011-*.md       ← Stage 11 (Test Run)
.ai/artifacts/run/MS-<id>-012-*.md       ← Stage 12 (User Approval) ⚠️ CRITICAL
.ai/artifacts/run/MS-<id>-013-*.md       ← Stage 13 (Release Manager)
```

**If any CRITICAL artifact is missing → workflow is INCOMPLETE → FAIL**

---

## For Claude/Codex Implementers

When executing `implement issue`:

1. **DO NOT judge task complexity** — Execute all 14 stages regardless
2. **DO NOT skip "obvious" stages** — All stages are required
3. **DO NOT optimize away stages** — Use the time, don't skip
4. **DO NOT assume** — Always run Stages 2, 3, 3.5 for questions
5. **DO verify all critical artifacts exist** — Stages 3.5, 7, 8, 10, 12 are mandatory
6. **DO ask user at Stage 12** — "Should we proceed to Stage 13?" — No auto-commit for ANY task
7. **DO execute Stage 13 only after user approval** — Never auto-create PR

---

## Quick Checklist Before Stage 13

```
❓ Stage 1 artifact exists? (Issue analysis)
❓ Stage 2 artifact exists? (Clarifications)
❓ Stage 3 artifact exists? (Implementation plan)
⚠️ Stage 3.5 artifact Status = PASS? (Constraints validated)
❓ Stage 5a artifact exists? (Failing tests written)
❓ Stage 5b artifact exists? (Implementation code)
❓ Stage 5c artifact exists? (Refactored code)
❓ Stage 6 artifact exists? (Build successful)
⚠️ Stage 7 artifact Status = PASS? (Security review)
⚠️ Stage 8 artifact Status = PASS? (Code review + coverage check)
❓ Stage 9 artifact exists or Status = SKIPPED? (Design review)
⚠️ Stage 10 artifact Status = PASS? (Coverage >= 80%)
❓ Stage 11 artifact exists? (All tests passed)
⚠️ Stage 12: User answered YES to "Should we proceed to Stage 13?"
```

**If ANY ⚠️ is missing/failed → STOP → Return to Stage 3 → Fix → Retry**

---

**Status:** This is a hard rule. No exceptions.
