# Codex Execution Strategy

Codex runs `run issue ...` by delegating all workflow logic to `.ai/WORKFLOW.md`.

## Rules

1. Do not duplicate pipeline logic in this file.
2. Execute stages exactly as defined in `.ai/WORKFLOW.md` and `.ai/PIPELINE.md`.
3. Use agents from `.ai/agents/`.
4. Keep execution automatic end-to-end.
5. Respect retry limits (max 2).
6. Emit required stage logs:
   - `Start stage <n>: <name>`
   - `Stage <n>: <name> completed`

## Artifacts

Write all stage artifacts to `.ai/artifacts/run/` using workflow naming.

Last updated: 2026-04-22
