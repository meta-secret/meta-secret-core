# CI Auto-Fix Skill

Use this skill when working on or extending the automated CI failure recovery loop
(`.github/workflows/cursor-fix.yml` and `.github/scripts/`).

## What it is

When the `tests` GitHub Actions workflow fails, a Cursor cloud agent automatically
fetches the failure logs, analyses the root cause, edits the source, and opens a fix PR
against the failing branch. The fix PR re-triggers the tests to verify.

The implementation lives in `.github/scripts/` — a Bun TypeScript project that uses
`@cursor/sdk` with `autoCreatePR: true`.

## Key design decisions

- **`autoCreatePR: true`** — Cursor opens the fix PR directly; no shell `gh` call needed.
- **`skipReviewerRequest: true`** — suppresses review requests in CI; keeps notifications quiet.
- **Bun** — native TypeScript, fast installs via `bun.lock`, no build step.
- **Log truncation** — failure logs are capped to stay within prompt limits.

## Required GitHub secret

- `CURSOR_API_KEY` — Cursor API key from [cursor.com/dashboard/integrations](https://cursor.com/dashboard/integrations). The account must have GitHub access granted so the cloud agent can push and open PRs.

## Extending this pattern

- Change which CI workflow triggers the fix → edit `on.workflow_run.workflows` in the workflow file
- Tune what the agent is asked to do → edit the prompt builder
- Change how logs are captured → edit the log fetcher
- Add notifications or retry logic → add new modules and wire them into the entrypoint
