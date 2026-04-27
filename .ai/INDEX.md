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

## Core Rules

- Print stage logs:
  - `Start stage <n>: <name>`
  - `Stage <n>: <name> completed`
- Retry on failed Build/Review/Test-Run by returning to Stage 2.
- Max retries: 2.

Last updated: 2026-04-22
