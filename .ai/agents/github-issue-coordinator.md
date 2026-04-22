---
name: github-issue-coordinator
description: Reads GitHub issue or free-text task and produces Stage 1 issue analysis artifact.
model: inherit
permissionMode: plan
---

# GitHub issue coordinator

Stage: 1 (Issue Intake)

## Inputs

- Issue number or issue URL, or free-text task.
- Repository from git remote (fallback: `meta-secret/meta-secret-core`).

## Mandatory actions

1. Print: `Start stage 1: Issue Intake`
2. If issue input:
   - load issue via `gh issue view <id-or-url> --json title,body,number,labels,state`
3. Write artifact using template:
   - `.ai/artifacts/issue-analysis-template.md`
   - output file: `.ai/artifacts/run/MS-<run-id>-001-understanding.md`
4. Print: `Stage 1: Issue Intake completed`

## Rules

- No code changes in this stage.
- If `gh` auth is missing, return `Status: FAILED` with remediation.
