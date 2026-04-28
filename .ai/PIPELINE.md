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
  - `bump_type`: `patch` | `minor` | `major`
  - `bump_rationale`: short justification for selected bump type
  - `target_version_files`: concrete files that must be version-bumped

## Stage 3: Implementation

- Agent: `code-implementer`
- Output: `MS-<run-id>-003-implementation.md`
- Must include:
  - crates/files changed
  - plan deviations (if any)

## Stage 4: Build (no tests)

- Command: from `meta-secret/` run `cargo build --workspace`
- Timeout: 600 seconds
- Output: `MS-<run-id>-004-build.md`
- Template: `build-report-template.md`
- Pass condition: `Status: PASSED`

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
- Command: from `meta-secret/` run `cargo test -p meta-secret-core -p meta-secret-cli -p meta-cli -p meta-secret-tests -p meta-secret-wasm`
- Output: `MS-<run-id>-007-test-run.md`
- Template: `test-report-template.md`
- Pass condition: `Status: PASSED`

## Stage 8: Branch + Commit + PR

- Agent: `release-manager`
- Output: `MS-<run-id>-008-pr.md`
- Must include:
  - Version gate check based on Stage 2 bump decision and git diff
  - Version audit block:
    - web old/new version (if applicable)
    - server old/new version (if applicable)
    - reason for bump
    - policy compliance confirmation

## SemVer Policy

- `patch`: bugfix/refactor without public contract changes
- `minor`: backward-compatible new functionality
- `major`: breaking API/FFI/protocol/storage/public behavior changes

Default version targets:

- `meta-secret/web-cli/ui/package.json`
- `meta-secret/meta-server/web-server/Cargo.toml`

Consistency rule:

- If both web and server are changed for one user-visible feature, both files must be bumped in the same run.
- If only one side is bumped, Stage 2 artifact must explain why the other side is excluded.

## Pre-Stage-8 Version Gate

Fail the pipeline and return to Stage 2 when:

- `bump_type` exists but listed version target files are unchanged
- version file changed but `bump_type` is missing
- declared bump type is inconsistent with detected change category

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

Each stage must print:
- `Start stage <n>: <name>`
- `Stage <n>: <name> completed`

## Failure Markers

- `Status: FAILED`
- `Return to Planning: YES`
- `**FAIL**`
- `FAIL`

Last updated: 2026-04-22
