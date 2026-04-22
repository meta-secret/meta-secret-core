# Claude Pipeline Strategy

Claude executes `run issue ...` using `.ai/WORKFLOW.md` as the source of truth.

## Rules

1. Do not duplicate stage prompts here.
2. Follow `.ai/WORKFLOW.md` and `.ai/PIPELINE.md` exactly.
3. Use agents from `.ai/agents/`.
4. Keep flow automatic, including retry loops.
5. Respect retry limits (max 2).
6. Emit required stage logs:
   - `Start stage <n>: <name>`
   - `Stage <n>: <name> completed`

## Artifacts

Write all stage artifacts to `.ai/artifacts/run/` with naming from `.ai/WORKFLOW.md`.

Last updated: 2026-04-22
