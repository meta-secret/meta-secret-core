---
description: Start delivery from a manual feature/bug description — task brief then plan; no GitLab issue required.
---

# Workflow from prompt

Arguments: free-text task description. Example: `/workflow-from-prompt Fix crash when opening vault on Android`

1. Apply skill **workflow-manual-task-brief** (`.claude/skills/workflow-manual-task-brief/`) and fill **manual-task-brief-template.md** from the user text.
2. **Stop.** Wait for user approval of the task brief (edit if needed).
3. Run **feature-planner** with the approved brief as input.
4. Continue the pipeline per [WORKFLOW.md](../WORKFLOW.md) after plan approval.
