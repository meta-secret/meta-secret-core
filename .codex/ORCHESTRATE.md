# Codex CLI Orchestration Entry

Use this file only as Codex-specific bootstrap.

## Mandatory

1. Parse user command: `run issue <id-or-text> [--from stage-N]`
2. Read and execute `.ai/WORKFLOW.md` as the single orchestration source
3. Use `.ai/PIPELINE.md` for stage-level details
4. Load stage agents from `.ai/agents/`

## Codex-specific execution notes

- Keep pipeline automatic; do not ask for confirmation between stages.
- Respect retry policy from `.ai/WORKFLOW.md` (max 2).
- For each stage print exact log lines:
  - `Start stage <n>: <name>`
  - `Stage <n>: <name> completed`

## Artifacts

- Write all artifacts to `.ai/artifacts/run/` with naming defined in `.ai/WORKFLOW.md`.

Last updated: 2026-04-22
