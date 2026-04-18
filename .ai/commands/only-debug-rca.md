---
description: Run debug-rca only — Plan mode, formatted RCA, next-step hints.
---

# Only debug RCA

Arguments: logs, stack trace, or failure description. Example: `/only-debug-rca <paste error>`

Delegate to subagent **debug-rca** with input: `$ARGUMENTS`

## Session mode

- **Use Plan mode** — default RCA is **diagnosis without repo writes** (see agent: no `Write`/`Edit` unless user explicitly expands scope).
- **Pause:** Present hypotheses and evidence; **wait for user confirmation** before recommending or applying code changes in a **follow-up** implementer turn.

## Presentation (required)

When presenting RCA:

1. Use **Markdown** with **emoji section headers** (examples: microscope for hypotheses, link for evidence, target for smallest next step).
2. **Bold** the **root cause** once identified; keep repro steps in a numbered list.
3. Separate **facts** vs **inferences** clearly.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-planner` | `/only-planner` | If fix needs a new or revised plan |
| `/core-only-implementer` | `/only-implementer` | Apply minimal fix after user ok |
| `/core-only-test-verifier` | `/only-test-verifier` | Re-verify after a fix |

Typical next step: **`/core-only-planner`** or **`/core-only-implementer`** after the user agrees with the RCA.

See [WORKFLOW.md](../WORKFLOW.md).
