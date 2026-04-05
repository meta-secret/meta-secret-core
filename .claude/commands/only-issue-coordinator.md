---
description: Run github-issue-coordinator only — Plan mode, formatted Summary, next-step hints (GitHub / gh).
---

# Only issue coordinator

Arguments: GitHub issue number or URL. Example: `/only-issue-coordinator 81`

Delegate to subagent **github-issue-coordinator** with input: `$ARGUMENTS`

Use skill **workflow-issue-handoff** to format the **Summary** for the planner.

## Session mode

- **Use Plan mode** — fetch and summarize **only**; no implementation; no git writes (see agent).
- **Pause:** Present the **Summary** and wait for user **approval** before planning or coding.

## Presentation (required)

When presenting the GitHub issue Summary:

1. Use **Markdown** with **emoji section labels** (ticket, memo, checkmark, warning as appropriate).
2. **Bold** issue id and title; bullets for labels and acceptance.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-planner` | `/only-planner` | Plan from the approved Summary |

Typical next step: **`/core-only-planner`** (MetaSecret) or **`/only-planner`** (repo root) with the approved Summary text.

See [WORKFLOW.md](../WORKFLOW.md).
