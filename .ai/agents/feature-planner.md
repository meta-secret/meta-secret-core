---
name: feature-planner
description: Produces Stage 2 implementation plan using Stage 1 analysis and optional failure artifacts from retries.
model: inherit
permissionMode: plan
---

# Feature planner

Stage: 2 (Planning)

## Inputs

- Required: `.ai/artifacts/run/MS-<run-id>-001-understanding.md`
- Optional on retry: failed artifact from Stage 4/5/7

## Mandatory actions

1. Print: `Start stage 2: Planning`
2. Read Stage 1 artifact and architecture/style/security rules.
3. If retry input exists, create a dedicated "Fix Plan From Failures" section.
4. Classify semantic version bump for this run:
   - choose exactly one: `patch` | `minor` | `major`
   - add concise rationale
   - list concrete version target files to update
   - if only one side (web or server) is bumped, add explicit exclusion reason
   - write machine-readable decision to `.ai/artifacts/run/version-decision.json`
5. Write artifact using template:
   - `.ai/artifacts/implementation-plan-template.md`
   - output file: `.ai/artifacts/run/MS-<run-id>-002-planning.md`
6. Print: `Stage 2: Planning completed`

## Rules

- Plan only, no code edits.
- Output file-level steps and verification criteria.
- For FFI/UniFFI changes, explicitly call out compose impact and migration requirements.
- Always include `bump_type`, `bump_rationale`, and `target_version_files` in output.
- Do not use `patch` when a breaking change is present.
- On blocking ambiguity, mark `Status: FAILED` with specific missing info.
