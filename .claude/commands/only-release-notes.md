---
description: Run release-notes only — Plan for draft text; Agent if writing files; formatted output, next-step hints.
---

# Only release notes

Arguments: diff summary, commit list, or topic. Example: `/only-release-notes Summarize recent commits on branch`

Delegate to subagent **release-notes** with input: `$ARGUMENTS`

Use skill **workflow-mr-body** for MR templates. See [WORKFLOW.md](../WORKFLOW.md).

## Session mode

- **Default: Plan mode** — the **release-notes** subagent outputs **text in chat** only (no `CHANGELOG` / file edits unless the user explicitly asks in this turn).
- **After user approves the draft:** if they want files updated (`CHANGELOG.md`, etc.), switch to **Agent mode** for that **explicit** write step only.

## Presentation (required)

When presenting release notes / MR copy:

1. Use **Markdown** with **emoji section headers** (examples: ship for title, memo for user-facing notes, lock for security-related lines).
2. **Bold** MR title suggestion; use bullets for **user-visible** changes vs internal.
3. Keep tone aligned with `PROJECT_CONTEXT.md` / `SECURITY.md`.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-release-manager` | `/only-release-manager` | Branch, commit, push, MR — **only after explicit user ok** |
| `/core-only-reviewer` | `/only-reviewer` | Final review before merge |

Typical next step after notes are approved: **`/core-only-release-manager`** (MetaSecret) or **`/only-release-manager`** (repo root).

See [WORKFLOW.md](../WORKFLOW.md).
