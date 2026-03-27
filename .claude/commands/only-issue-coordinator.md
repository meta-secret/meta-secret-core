---
description: Run only the gitlab-issue-coordinator subagent (glab issue view + handoff instructions).
---

# Only issue coordinator

Arguments: GitLab issue number or URL. Example: `/only-issue-coordinator 12`

Delegate to subagent **gitlab-issue-coordinator** with input: `$ARGUMENTS`

Use skill **workflow-issue-handoff** to format output for the planner.
