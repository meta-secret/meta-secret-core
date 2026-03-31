---
description: Run workflow-pattern-capture only — Plan mode, formatted proposals, next-step hints.
---

# Only workflow pattern capture

Arguments: context (e.g. repeated review feedback, error class, or “after large feature X”). Example: `/only-workflow-pattern-capture Same FFI boundary mistake in last 3 reviews`

Delegate to subagent **workflow-pattern-capture** with input: `$ARGUMENTS`

Use skill **workflow-pattern-capture** for triggers and output shape. See [WORKFLOW.md](../WORKFLOW.md).

## Session mode

- **Use Plan mode** — default output is **0–2 text proposals** (skill/command/rule); **no repo writes** unless the user explicitly asks to apply a change in this turn.
- **Pause:** If proposals need product/owner buy-in, **stop** and wait for the user before implementing anything.

## Presentation (required)

When presenting results:

1. Use **Markdown** with **emoji section headers** (examples: sparkles for proposals, no-entry if “no change”).
2. List **triggers** satisfied; cap output at **0–2** concrete proposals per the skill.
3. If **No changes recommended**, state that in **one clear line** with a brief reason.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-planner` | `/only-planner` | If a proposal requires a feature plan |
| `/core-only-implementer` | `/only-implementer` | If user approved a small concrete edit |

Often this command is a **terminal** optional step — return to normal delivery only if a proposal was accepted.

See [WORKFLOW.md](../WORKFLOW.md).
