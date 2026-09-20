# CI Auto-Fix Skill

Use this skill when working on or extending the automated CI failure recovery loop
(`.github/workflows/cursor-fix.yml` and `.github/scripts/`).
For classification, redaction, bounded retries, and trusted-branch gating,
also follow `.ai/skills/ci-monitor/SKILL.md`.

## What it is

When the `CI` GitHub Actions workflow fails, `CI Monitor` classifies the jobs,
retries transient infrastructure failures within a two-retry budget, and starts
Cursor only for trusted same-repository source failures. Sensitive, permission,
mixed, unknown, fork, and deployment failures are notify-only.

The implementation lives in `.github/scripts/` — a Bun TypeScript project that uses
`@cursor/sdk` with `autoCreatePR: true`.

## Key design decisions

- **`autoCreatePR: true`** — Cursor opens the fix PR directly; no shell `gh` call needed.
- **`skipReviewerRequest: true`** — suppresses review requests in CI; keeps notifications quiet.
- **Bun** — native TypeScript, fast installs via `bun.lock`, no build step.
- **Log truncation and redaction** — logs are bounded and sanitized before
  artifacts or Cursor prompts; raw logs are never uploaded.
- **Exact SHA guard** — the target branch is verified against the failing SHA
  because the Cursor SDK accepts a branch rather than a commit pin.

## Required GitHub secret

- `CURSOR_API_KEY` — Cursor API key from [cursor.com/dashboard/integrations](https://cursor.com/dashboard/integrations). The account must have GitHub access granted so the cloud agent can push and open PRs.

## Extending this pattern

- Change which CI workflow triggers the monitor → edit `on.workflow_run.workflows` in the workflow file
- Tune classification/retry/redaction → edit `.github/scripts/lib/ci-monitor.ts`
- Tune what the agent is asked to do → edit the prompt builder
- Change how logs are captured → edit `ci-monitor.ts`; preserve redaction before persistence
- Add notifications or retry logic → add a least-privilege workflow job and tests
