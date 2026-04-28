# Quick Start

## Run full flow

- `run issue 123`
- `run issue "my custom task"`

## Resume from stage

- `run issue 123 --from stage-4`

## Source of truth

- `.ai/WORKFLOW.md`
- `.ai/PIPELINE.md`

## Stage list

1. Issue Intake
2. Planning
3. Implementation
4. Build (no tests, 10 min timeout)
5. Code Review
6. Test Authoring
7. Test Run
8. Branch + Commit + PR

## Artifacts

- `.ai/artifacts/run/MS-<run-id>-<stage>-<name>.md`

## Mandatory stage logs

- Start: emoji per stage (table in `.ai/WORKFLOW.md`) + `Start stage <n>: <name>`
- `✅ Stage <n>: <name> completed`

Last updated: 2026-04-28
