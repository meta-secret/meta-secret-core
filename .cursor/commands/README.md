# Cursor commands (workflow entry)

Cursor runs delivery via the workflow entry files, not via duplicated local agent folders.

Single source of truth:
- `.ai/WORKFLOW.md`
- `.ai/PIPELINE.md`

Use in Cursor chat:
- `run issue <id-or-text>`
- `run issue <id-or-text> --from stage-<n>`

Issue intake in this repo is GitHub-only (`gh`) through `github-issue-coordinator`.
