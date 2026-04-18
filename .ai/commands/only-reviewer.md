---
description: Run code-reviewer only — Plan mode, formatted review, next-step hints.
---

# Only reviewer

Arguments: optional scope (files, or “review last changes”). Example: `/only-reviewer`

Delegate to subagent **code-reviewer** with input: `$ARGUMENTS`

## Session mode

- **Use Plan mode** — review is **read-only** (no edits from this subagent).
- **Pause:** After the review, **stop** and wait for the user to **accept**, **dispute**, or **prioritize** findings before suggesting or applying fixes elsewhere.

## Presentation (required)

When presenting the review:

1. Use **Markdown** — group by severity (must-fix vs nice-to-have) with **emoji** (examples: stop for blockers, warning for important, sparkles for optional).
2. **Bold** file:line references; use bullet lists; keep excerpts in **fenced code blocks** when quoting code.
3. End with a **short summary** paragraph (overall verdict).

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-implementer` | `/only-implementer` | Address must-fix review items |
| `/core-only-planner` | `/only-planner` | Rework plan if architecture must change |
| `/core-only-test-verifier` | `/only-test-verifier` | Re-run tests after fixes |

Typical next step if there are must-fix items: **`/core-only-implementer`** (MetaSecret) or **`/only-implementer`** (repo root).

See [WORKFLOW.md](../WORKFLOW.md).
