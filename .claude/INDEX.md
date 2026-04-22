# Claude Code Entry

Use `/run issue ...`.

Single source of truth:
- `.ai/WORKFLOW.md`
- `.ai/PIPELINE.md`

Stage order:
1. Issue Intake
2. Planning
3. Implementation
4. Build (no tests, max 10 min)
5. Code Review
6. Test Authoring
7. Test Run
8. Branch + Commit + PR

Required stage logs:
- `Start stage <n>: <name>`
- `Stage <n>: <name> completed`

Artifacts:
- `.ai/artifacts/run/MS-<run-id>-<stage>-<name>.md`

Entrypoint executor:
- `.claude/ORCHESTRATE.md`
