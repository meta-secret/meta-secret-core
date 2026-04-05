---
description: Start delivery from a manual feature/bug description — task brief then plan; no GitHub issue required.
---

# Only from prompt

Arguments: free-text task description. Example: `/only-from-prompt Fix crash when opening vault on Android`

1. Apply skill **workflow-manual-task-brief** (`.claude/skills/workflow-manual-task-brief/`) and fill **manual-task-brief-template.md** from the user text.
2. **Stop.** Wait for user approval of the task brief (edit if needed).
3. Run **feature-planner** with the approved brief as input.
4. Continue the pipeline per [WORKFLOW.md](../WORKFLOW.md) after plan approval.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-planner` | `/only-planner` | Re-plan or refine if the brief was wrong |
| `/core-only-implementer` | `/only-implementer` | After plan approval |

Typical path after brief approval: **`/core-only-planner`** (MetaSecret) or **`/only-planner`** (repo root) with the approved brief.

See [WORKFLOW.md](../WORKFLOW.md).
