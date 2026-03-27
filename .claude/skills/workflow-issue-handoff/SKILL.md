---
name: workflow-issue-handoff
description: Format a GitHub (or GitLab) issue summary for the feature-planner and later phases after gh issue view.
---

# Workflow — issue handoff

Use after fetching an issue. Produce a single structured block the user can approve before planning.

## Fetch the issue

**Primary (GitHub):**
```
gh issue view <n> --repo <owner/repo>
```

**Secondary (GitLab, if applicable):**
```
glab issue view <n>
```

## Read first

- [WORKFLOW.md](../../../WORKFLOW.md) at repo root
- [issue-handoff-template.md](issue-handoff-template.md)

## Output

Fill every section in the template. Do not implement code in this skill—summary only.
