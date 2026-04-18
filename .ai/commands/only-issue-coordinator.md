---
description: Run gitlab-issue-coordinator only — Plan mode, formatted Summary, next-step hints (GitLab / glab).
---

# Only issue coordinator

Arguments: GitLab issue number or URL. Example: `/only-issue-coordinator 12`

Delegate to subagent **gitlab-issue-coordinator** with input: `$ARGUMENTS`

Use skill **workflow-issue-handoff** to format the **Summary** for the planner.

## Session mode

- **Use Plan mode** — fetch and summarize **only**; no implementation; no git writes (see agent).
- **Pause:** Present the **Summary** and wait for user **approval** before planning or coding.

## Presentation (required)

When presenting the GitLab issue Summary:

1. Use **Markdown** with **emoji section labels** (same spirit as `workflow-from-issue`: ticket, memo, checkmark, warning as appropriate).
2. **Bold** issue id and title; bullets for labels and acceptance.
3. Clarify if the tracker is **GitLab** (`glab`) vs **GitHub** (use **`workflow-from-issue`** / **`github-issue-coordinator`** for GitHub).

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-planner` | `/only-planner` | Plan from the approved Summary |

Typical next step: **`/core-only-planner`** (MetaSecret) or **`/only-planner`** (repo root) with the approved Summary text.

See [WORKFLOW.md](../WORKFLOW.md).
