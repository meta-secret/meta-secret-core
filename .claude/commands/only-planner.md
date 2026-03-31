---
description: Run feature-planner only — Plan mode, formatted plan output, next-step hints.
---

# Only planner

Arguments: context for the plan (issue Summary, task brief, or user notes). Example: `/only-planner <paste context>`

Delegate to subagent **feature-planner** with input: `$ARGUMENTS`

Use skill **workflow-plan-output** for expected plan shape. See [WORKFLOW.md](../WORKFLOW.md).

## Session mode

- **Use Plan mode** for this session (no repo writes from the planner subagent).
- **Pause:** After you output the plan, **stop** and wait for the user to **approve, edit, or reject** the plan before any implementation.

## Presentation (required)

When presenting the plan to the user:

1. Use **Markdown** — `##` / `###` headings, **bold** for goals, constraints, and decisions.
2. Add **emoji section labels** for scanability (examples: target for scope, memo for approach, warning for risks, checklist for steps).
3. Keep the plan **actionable** and aligned with **workflow-plan-output**; avoid dumping raw file trees unless needed.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-implementer` | `/only-implementer` | Apply the **approved** plan in code |
| `/core-only-reviewer` | `/only-reviewer` | Review a change set before merge |
| `/core-only-debug-rca` | `/only-debug-rca` | If requirements are unclear or blocked by an error |

Typical next step after plan approval: **`/core-only-implementer`** (MetaSecret) or **`/only-implementer`** (repo root).

See [WORKFLOW.md](../WORKFLOW.md).
