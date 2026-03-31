---
description: Run release-manager only — Agent mode, formatted status, next-step hints; never commit/push without explicit ok.
---

# Only release manager

Arguments: branch name or convention hint. Example: `/only-release-manager feature/foo`

Delegate to subagent **release-manager** with input: `$ARGUMENTS`

Reminder: **never** commit or push without explicit user approval in this session.

## Session mode

- **Use Agent mode** — git branch/commit/push and optional `glab` MR require **git write** and shell.
- **Mandatory pauses:** follow the **release-manager** agent: **stop before `git commit`** and **again before `git push`** for explicit **“ok”** from the user.

## Presentation (required)

When reporting to the user:

1. Use **Markdown** with **emoji section headers** (examples: branch for branch name, package for staged paths, rocket for push status).
2. **Bold** branch names and remote; list **exact commands** the user must run if automation stops.
3. Never present a push as done without confirmation.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-workflow-pattern-capture` | `/only-workflow-pattern-capture` | Optional: capture repeated process learnings |
| `/core-only-release-notes` | `/only-release-notes` | Refine MR body if needed |

Typical next step after MR is open: optional **`/core-only-workflow-pattern-capture`**, or close the task per team process.

See [WORKFLOW.md](../WORKFLOW.md).
