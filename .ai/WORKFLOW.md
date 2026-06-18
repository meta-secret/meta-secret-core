# Automated Workflow Orchestration

Single source of truth for `run issue <id>` / `run issue "<text>"` across Claude Code, Cursor, and Codex CLI.

## Command Contract

- Trigger: `run issue <id-or-text>`
- Optional resume: `run issue <id-or-text> --from stage-<n>`
- Artifacts directory: `.ai/artifacts/run/`
- Artifact naming: `MS-<run-id>-<stage-number>-<stage-name>[ -retry-N ].md`
- Retry budget: `2` full fix loops

`<run-id>` rules:
- Numeric issue input: use issue number (`123`)
- Free-text input: use UTC timestamp (`YYYYMMDDHHmmss`)

## Required Stage Logs

**Finish (every stage):** `✅ Stage <n>: <name> completed`

**Start:** exactly one leading emoji per stage (table below), then one ASCII space, then `Start stage <n>: <name>`.

| Stage | Name | Start line begins with |
|------:|------|------------------------|
| 1 | Issue Intake | `📋 ` |
| 2 | Planning | `🗺️ ` |
| 3 | Implementation | `🛠️ ` |
| 4 | Build | `🏗️ ` |
| 5 | Code Review | `🔍 ` |
| 6 | Test Authoring | `🧪 ` |
| 7 | Test Run | `▶️ ` |
| 8 | Branch + Commit + PR | `🚀 ` |

After the start emoji there must be exactly one ASCII space before `Start stage`. Substrings `Start stage` and `completed` remain grep-friendly for tooling.

Examples:
- `🏗️ Start stage 4: Build`
- `✅ Stage 4: Build completed`

## 8-Stage Pipeline

1. Stage 1: Issue Intake
2. Stage 2: Planning
3. Stage 3: Implementation
4. Stage 4: Build (no tests, max 10 minutes)
5. Stage 5: Code Review
6. Stage 6: Test Authoring
7. Stage 7: Test Run
8. Stage 8: Branch + Commit + PR

## Stage Specs

### Stage 1: Issue Intake

Primary agent: `github-issue-coordinator`
Template: `.ai/artifacts/issue-analysis-template.md`
Output: `.ai/artifacts/run/MS-<run-id>-001-understanding.md`

Required behavior:
- Read issue (or free-text task)
- Produce output following issue-analysis template

### Stage 2: Planning

Agent: `feature-planner`
Template: `.ai/artifacts/implementation-plan-template.md`
Input:
- Stage 1 artifact
- If retry: failed artifact from Stage 4/5/7
Output: `.ai/artifacts/run/MS-<run-id>-002-planning.md`

Required behavior:
- Create implementation plan aligned with core architecture/security/style
- If retry, add explicit fix plan derived from failure artifact

### Stage 3: Implementation

Agent: `code-implementer`
Input:
- Stage 2 artifact
Output:
- `.ai/artifacts/run/MS-<run-id>-003-implementation.md`

Required behavior:
- Implement approved plan with minimal diff
- Respect Rust crate boundaries and FFI stability rules

### Stage 4: Build (no tests)

Command:
- from `meta-secret/`: `cargo build --workspace`

Timeout:
- hard limit 10 minutes (600 seconds)

Template: `.ai/artifacts/build-report-template.md`
Output: `.ai/artifacts/run/MS-<run-id>-004-build.md`

Required behavior:
- Capture command, duration, and status
- Mark `Status: PASSED` or `Status: FAILED`

### Stage 5: Code Review

Agent: `code-reviewer`
Template: `.ai/artifacts/review-report-template.md`
Input: code diff + architecture/style/security rules
Output: `.ai/artifacts/run/MS-<run-id>-005-review.md`

Required behavior:
- Output `Status: PASSED` or `Status: FAILED`
- When failed: include concrete blocking issues

### Stage 6: Test Authoring

Agent: `test-author`
Input:
- Stage 3 implementation artifact
- Stage 5 findings (if any)
Output: `.ai/artifacts/run/MS-<run-id>-006-testing.md`

Required behavior:
- Add/update automated tests for changed behavior
- Cover edge cases from plan and review feedback

### Stage 7: Test Run

Agent: `test-verifier`
Template: `.ai/artifacts/test-report-template.md`
Command suggestion:
- from `meta-secret/`: `cargo test -p meta-secret-core -p meta-secret-cli -p meta-cli -p meta-secret-tests -p meta-secret-wasm`
Output: `.ai/artifacts/run/MS-<run-id>-007-test-run.md`

Required behavior:
- Output `Status: PASSED` or `Status: FAILED`
- Include failed test list and root-cause summary

### Stage 8: Branch + Commit + PR

Agent: `release-manager`
Output: `.ai/artifacts/run/MS-<run-id>-008-pr.md`

Required behavior:
- Create branch: `{Prefix}/kuklin/MS-{issueNumber}` for numeric issues (see release-manager policy)
- Commit and push with explicit user approvals
- Open PR to `main`

## Automatic Recovery Loops

If any of these stages fails:
- Stage 4 (Build)
- Stage 5 (Code Review)
- Stage 7 (Test Run)

Then run recovery loop:

1. Feed failed artifact into Stage 2 planning as mandatory context
2. Re-run Stage 3 -> Stage 4 -> Stage 5 -> Stage 6 -> Stage 7
3. Stop when all pass, then continue to Stage 8
4. Max retries: 2

On retry artifacts, append `-retry-1` / `-retry-2`.

## Failure Markers

Pipeline must stop if artifact contains any marker:

- `Status: FAILED`
- `Return to Planning: YES`
- `**FAIL**`
- `FAIL`
- `❌`

## IDE Entry Points

- Claude Code: `.claude/ORCHESTRATE.md`
- Cursor: `.cursor/WORKFLOW.md`
- Codex CLI: `.codex/ORCHESTRATE.md`

All entry points must delegate orchestration logic to this file to avoid duplication.

Last updated: 2026-04-28
