---
description: Start delivery from a GitHub issue — fetch with gh, format handoff, stop for approval before planning.
---

# Workflow from issue

Arguments: issue reference (number or URL). Example: `/workflow-from-issue 81`

1. Run the **github-issue-coordinator** subagent (MetaSecret-level: `.claude/agents/github-issue-coordinator.md`) with `TARGET_SUBDIR=meta-secret-core` and `ISSUE=$ARGUMENTS`.
   - If running from the **meta-secret-core** workspace directly (not MetaSecret root), call `gh issue view <n> --repo meta-secret/meta-secret-core` and format the result with skill `workflow-issue-handoff`.
2. Apply skill **workflow-issue-handoff** (`.claude/skills/workflow-issue-handoff/`) to format the summary.
3. **Stop.** Wait for explicit user approval of the issue summary.
4. Next: `/only-planner` with the approved handoff text (or delegate **feature-planner** with that context).

Read [WORKFLOW.md](../WORKFLOW.md) for the full pipeline.

> **Note:** Issues for this repo are on **GitHub** (`gh`). The `gitlab-issue-coordinator` agent exists for GitLab-hosted projects only.
