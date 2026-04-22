# Claude Code Orchestration Entry

Use this file only as Claude bootstrap for `/run issue ...`.

## Mandatory

1. Parse: `run issue <id-or-text> [--from stage-N]`
2. Execute `.ai/WORKFLOW.md` as single source of truth
3. Use `.ai/PIPELINE.md` for per-stage contract
4. Run agents from `.ai/agents/`

## Claude-specific execution notes

- Execute end-to-end automatically.
- Do not duplicate stage logic here.
- Respect retry policy (max 2) from `.ai/WORKFLOW.md`.
- For each stage print exact log lines:
  - `Start stage <n>: <name>`
  - `Stage <n>: <name> completed`

## Artifacts

- Write all artifacts to `.ai/artifacts/run/` using workflow naming.

Last updated: 2026-04-22
