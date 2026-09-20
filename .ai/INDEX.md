# AI System Index

Single source of truth for automation in this repo.

## Start Here

1. `.ai/README.md`
2. `.ai/WORKFLOW.md`
3. `.ai/PIPELINE.md`

## IDE Entrypoints

- Claude: `.claude/ORCHESTRATE.md`
- Cursor: `.cursor/WORKFLOW.md`
- Codex: `.codex/ORCHESTRATE.md`

All three must delegate to `.ai/WORKFLOW.md`.

## Stage Agents

- `github-issue-coordinator`
- `feature-planner`
- `code-implementer`
- `code-reviewer`
- `test-author`
- `test-verifier`
- `release-manager`

## Artifacts

- Directory: `.ai/artifacts/run/`
- Naming: `MS-<run-id>-<stage-number>-<stage-name>.md`
- Templates: `.ai/artifacts/*-template.md`

## Skills

- **Build / test / Docker:** [`.ai/skills/build-via-task/SKILL.md`](skills/build-via-task/SKILL.md) — mandatory before `task` or verification commands
- **PR title & description:** [`.ai/skills/workflow-mr-body/SKILL.md`](skills/workflow-mr-body/SKILL.md) — full branch scope; `gh pr create` / **`gh pr edit`**
- **Final UI E2E:** root [`ui-e2e-testing`](../../.ai/skills/ui-e2e-testing/SKILL.md) skill and [`ui-e2e-test-contract`](../../.ai/rules/ui-e2e-test-contract.md) — after regular checks and before release

## Core Rules

- Print stage logs (start emoji per stage — `.ai/WORKFLOW.md`):
  - `<stage-start-emoji> Start stage <n>: <name>`
  - `✅ Stage <n>: <name> completed`
- Retry on failed Build/Review/Test-Run by returning to Stage 2.
- Max retries: 2.

Last updated: 2026-04-28
