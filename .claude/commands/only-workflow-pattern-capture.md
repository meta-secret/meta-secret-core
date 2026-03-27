---
description: Run only the workflow-pattern-capture subagent (0–2 process suggestions; no repo writes by default).
---

# Only workflow pattern capture

Arguments: context (e.g. repeated review feedback, error class, or “after large feature X”). Example: `/only-workflow-pattern-capture Same FFI boundary mistake in last 3 reviews`

Delegate to subagent **workflow-pattern-capture** with input: `$ARGUMENTS`

Use skill **workflow-pattern-capture** for triggers and output shape. See [WORKFLOW.md](../WORKFLOW.md).
