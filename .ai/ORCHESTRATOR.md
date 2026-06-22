# AI Orchestrator — meta-secret-core

Command routing and execution for meta-secret-core Rust backend.

---

## Command Routing

### Full Workflow

#### `implement issue <payload>`

Execute:
1. **FIRST:** Read `.ai/rules/implement-issue-execution-checklist.md` (MANDATORY)
2. **SECOND:** Read `.ai/rules/no-stage-skipping-even-for-simple-tasks.md` (ENFORCEMENT)
3. Read `.ai/commands/implement-issue.md`
4. Execute ALL 14 stages in strict order: 1 → 2 → 3 → 3.5 → 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10 → 11 → 12
5. Do NOT skip Stage 6 (Security Review), Stage 7 (Code Review), Stage 10 (Coverage)
6. For Stage 12: Use AskUserQuestion tool to get approval before PR
7. Create artifacts for ALL stages in `.ai/artifacts/run/`

---

## Single Source of Truth

Operational workflow must be taken from:
- `.ai/WORKFLOW.md` (complete 14-stage specification)
- `.ai/commands/implement-issue.md` (command details)
- `.ai/CONSTRAINTS.md` (architecture rules)

Do not duplicate stage logic in this file or IDE-specific files.

---

## Stage Agents

| Stage | Agent | Input | Output |
|---|---|---|---|
| 1 | github-issue-coordinator | Issue/URL/text | Issue analysis artifact |
| 2 | requirements-clarifier | Issue + Stage 1 | Clarifications artifact |
| 3 | feature-planner | All previous | Implementation plan |
| 3.5 | constraint-validator | Plan | Validation Pass/Fail |
| 5a | tdd-test-author | Plan | Failing tests |
| 5b | tdd-implementer | Tests | Implementation |
| 5c | tdd-refactorer | Implementation | Refactored code |
| 6 | cargo build | Code | Build report |
| 7 | security-reviewer | Code | Security audit report |
| 8 | code-reviewer | Code + build | Code review findings |
| 9 | design-reviewer | Architecture | Design review or Skipped |
| 10 | coverage-verifier | Code | Coverage report (>= 80%) |
| 11 | test-runner | Code | Test results |
| 12 | release-manager | All approved | PR created |

---

## Artifacts

All artifacts must be written to:
- `.ai/artifacts/run/MS-<run-id>-<stage>-<name>.md`

Naming convention:
- `MS-<run-id>-<stage-number>-<stage-name>[ -retry-N ].md`
- Example: `MS-42-010-coverage.md` or `MS-42-010-coverage -retry-1.md`

Every artifact MUST have:
```markdown
**Status:** Success | Failed | Skipped
```

---

## Recovery Policy

If any stage fails:
1. Identify root cause in failed artifact
2. Document: `Status: Failed` + reason
3. Run `debug-rca` agent to analyze root cause
4. Return to Stage 3 (Planning) with failed artifact as input
5. Create fix plan based on failure
6. Re-execute stages 5a → 5b → 5c → 6 → 7 → 8 → 9 → 10 → 11 → 12
7. Max retries: 2 full loops

**Critical failures (block recovery):**
- Stage 3.5 (Constraints violated) → must re-architect, not just code fix
- Stage 7 (Security issue) → must review crypto correctness, may need design change

---

## Build & Test Commands

**Stage 6: Build**
```bash
cargo build --release
```

**Stage 7: Security Review**
```bash
cargo clippy --all-targets -- -D warnings
grep -n "unsafe" src/ crates/*/src/
# Manual crypto audit if crypto modules changed
```

**Stage 10: Coverage**
```bash
cargo tarpaulin --out Html --timeout 300 --fail-under 80
```

**Stage 11: Tests**
```bash
cargo test --all
```

---

## Failure Markers

- `Status: FAILED`
- `Return to Planning: YES`
- `**FAIL**` or `FAIL` in artifact content

---

Last updated: 2026-06-22
