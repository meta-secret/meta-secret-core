# Cursor Workflow Entry

Use this file as Cursor bootstrap for `run issue ...`.

## Mandatory

1. Parse: `run issue <id-or-text> [--from stage-N]`
2. Execute `.ai/WORKFLOW.md` (single source of truth)
3. Use `.ai/PIPELINE.md` for stage details
4. Run agents from `.ai/agents/`

## Cursor-specific execution notes

- Keep flow automatic from stage to stage.
- Do not maintain duplicated stage prompts in this file.
- Respect retry policy (max 2) from `.ai/WORKFLOW.md`.
- For each stage print exact log lines:
  - `Start stage <n>: <name>`
  - `Stage <n>: <name> completed`

## Artifacts

- Write all artifacts to `.ai/artifacts/run/` using workflow naming.

Last updated: 2026-04-22
