---
name: workflow-mr-body
description: Checklist for merge request title and description before release-manager creates the MR.
---

# Workflow — MR body

Use with **release-notes** subagent output. Keeps MR text consistent with [SECURITY.md](../../../SECURITY.md) (no secrets in text).

## Read first

- [mr-body-template.md](mr-body-template.md)

## Rules

- Summarize user-visible changes and risk
- List test commands run or “not run (reason)”
- Do not paste tokens or internal-only URLs unless the user provided them for the MR
