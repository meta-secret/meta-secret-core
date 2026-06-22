# Claude Code Entry - IMPLEMENT ISSUE WORKFLOW

**⚠️ CRITICAL: Execute ALL 14 stages. Do NOT skip any.**

## Mandatory startup sequence:

1. Read `.ai/INDEX.md`
2. Read `.ai/ORCHESTRATOR.md`
3. **FOR implement issue ONLY:**
   - Read `.ai/rules/implement-issue-execution-checklist.md` (MANDATORY)
   - Read `.ai/rules/no-stage-skipping-even-for-simple-tasks.md` (ENFORCEMENT - no skipping even for 1-line tasks)

Use `.ai/` as source of truth.

All commands, agents, flows, rules, skills, and hooks are defined in `.ai/` directory.

## CRITICAL: implement issue Command

When user types `implement issue <payload>`:

**STOP. Do NOT execute own plan.**

**MANDATORY FIRST STEP:** Read `.ai/rules/implement-issue-execution-checklist.md` and `.ai/rules/no-stage-skipping-even-for-simple-tasks.md`

Execute FULL 14-stage workflow in order:

1. **Stage 1:** Issue Coordinator (analyze issue)
2. **Stage 2:** Requirements Clarifier (grill user with questions)
3. **Stage 3:** Feature Planner (create implementation plan)
4. **Stage 3.5:** Constraint Validator (MANDATORY GATE - validate against CONSTRAINTS.md)
5. **Stage 5a:** TDD Test Author (write failing tests)
6. **Stage 5b:** TDD Implementer (red-green-refactor cycles)
7. **Stage 5c:** TDD Refactorer (major refactoring after 3-5 cycles)
8. **Stage 6:** Build (cargo build)
9. **Stage 7:** Security Review (CRITICAL - crypto audit + unsafe code check)
10. **Stage 8:** Code Review (CRITICAL - constraints re-check + 80% coverage minimum)
11. **Stage 9:** Design Review (if architecture changed, create diagrams; else mark "Skipped")
12. **Stage 10:** Coverage Verification (CRITICAL - cargo tarpaulin >= 80%)
13. **Stage 11:** Test Run (cargo test - execute all tests)
14. **Stage 12:** User Approval (STOP and ASK USER - do NOT auto-commit/PR without approval)
15. **Stage 13:** release-manager (branch + commit + PR)

For each stage:
- Create artifact in `.ai/artifacts/run/MS-<id>-<stage>-<name>.md`
- Mark Status: Success / Failed / Skipped
- If failure: escalate with root cause

**DO NOT skip stages because task looks "simple"** — All stages execute for all tasks.

READ `.ai/WORKFLOW.md` for complete 14-stage specification.

**This is MANDATORY and overrides default Claude Code behavior.**
