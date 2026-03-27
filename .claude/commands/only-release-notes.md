---
description: Run only the release-notes subagent (MR/changelog text, no git).
---

# Only release notes

Arguments: diff summary, commit list, or topic. Example: `/only-release-notes Summarize recent commits on branch`

Delegate to subagent **release-notes** with input: `$ARGUMENTS`

Use skill **workflow-mr-body** for MR templates. See [WORKFLOW.md](../WORKFLOW.md).
