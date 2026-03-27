---
description: Run only the release-manager subagent (branch, commit, push, MR — commit/push only after explicit user ok).
---

# Only release manager

Arguments: branch name or convention hint. Example: `/only-release-manager feature/foo`

Delegate to subagent **release-manager** with input: `$ARGUMENTS`

Reminder: **never** commit or push without explicit user approval in this session.
