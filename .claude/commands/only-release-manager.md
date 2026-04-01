---
description: Run release-manager only — Agent mode, formatted status, next-step hints; never commit/push without explicit ok.
---

# Only release manager

Arguments: **GitHub issue number or issue URL** (recommended when you are on `main` and want the default branch name), **or** an explicit branch name. Examples:

- `/only-release-manager 99` — on `main`, creates e.g. **`Task/kuklin/MS-99`** (prefix from `[Task]` / `[Feature]` / `[Bug]` in issue title) via `gh issue view`.
- `/only-release-manager https://github.com/org/repo/issues/42`
- `/only-release-manager feature/manual-branch` — skips automatic naming.

Delegate to subagent **release-manager** with input: `$ARGUMENTS`

See **[`.claude/agents/release-manager.md`](../agents/release-manager.md)** for the full branch naming rule (`{Task|Feature|Bug}/kuklin/MS-{N}` when on `main`), commit message from diff, and pauses.

Reminder: **never** commit or push without explicit user approval in this session.

## Session mode

- **Use Agent mode** — git branch/commit/push and optional `glab` / `gh` PR require **git write** and shell.
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
