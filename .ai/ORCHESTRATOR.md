# Orchestrator Notes

This file documents high-level orchestration ownership.

## Single Source of Truth

Operational workflow must be taken from:
- `.ai/WORKFLOW.md`
- `.ai/PIPELINE.md`

Do not duplicate stage logic in this file or IDE-specific files.

## Stage Agents

- `github-issue-coordinator`
- `feature-planner`
- `code-implementer`
- `code-reviewer`
- `test-author`
- `test-verifier`
- `release-manager`

## Artifacts

All artifacts must be written to:
- `.ai/artifacts/run/`

Naming:
- `MS-<run-id>-<stage-number>-<stage-name>.md`
- retry suffixes when applicable

## Recovery Policy

On failed build/review/test-run, return to Stage 2 with failed artifact as input.
Max retries: 2.

Last updated: 2026-04-22
