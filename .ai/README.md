# AI Automation — meta-secret-core

Automated issue-to-PR workflow shared by Claude Code, Cursor, and Codex CLI.

## Start

- `run issue 123`
- `run issue "my custom task"`
- `run issue 123 --from stage-4`

## Workflow

Defined in `.ai/WORKFLOW.md` and `.ai/PIPELINE.md`.

8 stages:
1. Issue Intake
2. Planning
3. Implementation
4. Build (no tests, timeout 10 min)
5. Code Review
6. Test Authoring
7. Test Run
8. Branch + Commit + PR

## Domain Skills and Rules

- Domain skills:
  - `.ai/skills/core-guardian/`
  - `.ai/skills/cli-guardian/`
  - `.ai/skills/web-guardian/`
  - `.ai/skills/mobile-lib-guardian/`
- Path-scoped rules:
  - `.ai/rules/domains/core.mdc`
  - `.ai/rules/domains/cli.mdc`
  - `.ai/rules/domains/web.mdc`
  - `.ai/rules/domains/mobile-lib.mdc`

## Required Stage Logs

- `Start stage <n>: <name>`
- `Stage <n>: <name> completed`

## Artifacts

- Location: `.ai/artifacts/run/`
- Naming: `MS-<run-id>-<stage>-<name>.md`
- Templates: `.ai/artifacts/*-template.md`

## Retry Policy

On failed Build/Review/Test-Run:
- return to Stage 2 with failed artifact as input
- max retries: 2

Last updated: 2026-04-22
