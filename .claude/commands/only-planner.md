---
description: Run only the feature-planner subagent (plan output, no implementation).
---

# Only planner

Arguments: context for the plan (issue handoff, task brief, or user notes). Example: `/only-planner <paste context>`

Delegate to subagent **feature-planner** with input: `$ARGUMENTS`

Use skill **workflow-plan-output** for expected plan shape. See [WORKFLOW.md](../WORKFLOW.md).
