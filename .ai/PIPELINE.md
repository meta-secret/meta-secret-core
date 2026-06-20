# Pipeline Specification

Detailed stage contract for `.ai/WORKFLOW.md`.

## Stage 1: Issue Intake

- Agent: `github-issue-coordinator`
- Input: issue id or free-text task
- Output: `MS-<run-id>-001-understanding.md`
- Template: `issue-analysis-template.md`
- Must include:
  - Problem, Goal, Requirements, Assumptions, Affected Areas

## Stage 2: Planning

- Agent: `feature-planner`
- Input:
  - Stage 1 artifact
  - Failed artifact on retries
- Output: `MS-<run-id>-002-planning.md`
- Template: `implementation-plan-template.md`
- Must include:
  - File-level execution plan
  - Risks and edge cases
  - Retry fix section when retrying

## Stage 3: Implementation

- Agent: `code-implementer`
- Output: `MS-<run-id>-003-implementation.md`
- Must include:
  - crates/files changed
  - plan deviations (if any)

## Stage 4: Build (no tests)

- Skill: `.ai/skills/build-via-task/SKILL.md` (mandatory)
- Command: from **repository root**, run the narrowest `task` target(s) for the change (e.g. `task web-local`, `task wasm-local`, `task test`)
- Timeout: 600 seconds
- Output: `MS-<run-id>-004-build.md`
- Template: `build-report-template.md`
- Pass condition: `Status: PASSED`

Forbidden: `docker buildx bake`, `docker buildx build`, `docker build` (use `task` or add a task first).

## Stage 5: Code Review

- Agent: `code-reviewer`
- Output: `MS-<run-id>-005-review.md`
- Template: `review-report-template.md`
- Pass condition: `Status: PASSED`

## Stage 6: Test Authoring

- Agent: `test-author`
- Output: `MS-<run-id>-006-testing.md`
- Inputs: implementation artifact + review findings

## Stage 7: Test Run

- Agent: `test-verifier`
- Skill: `.ai/skills/build-via-task/SKILL.md`
- Command: from **repository root**: `task test` (CI parity)
- Output: `MS-<run-id>-007-test-run.md`
- Template: `test-report-template.md`
- Pass condition: `Status: PASSED`

## Stage 8: Branch + Commit + PR

- Agent: `release-manager`
- Output: `MS-<run-id>-008-pr.md`

## Retry Rules

Retry trigger:
- Build failed (Stage 4)
- Code review failed (Stage 5)
- Test run failed (Stage 7)

Retry path:
- Return to Stage 2 with failed artifact as input
- Re-run pipeline from Stage 3 onward
- Max retries: 2

## Stage Log Contract

Each stage must print (see start-emoji table in `.ai/WORKFLOW.md`):
- Start: stage-specific emoji + `Start stage <n>: <name>`
- End: `✅ Stage <n>: <name> completed`

## Failure Markers

- `Status: FAILED`
- `Return to Planning: YES`
- `**FAIL**`
- `FAIL`

Last updated: 2026-04-28
