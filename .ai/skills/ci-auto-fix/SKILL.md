# CI Auto-Fix Skill

Use this skill when working on or extending the automated CI failure recovery loop
(`.github/workflows/cursor-fix.yml` and `.github/scripts/`).

## What it is

When the `tests` GitHub Actions workflow fails, a Cursor cloud agent automatically:

1. Fetches `gh run view --log-failed` output from the failed run.
2. Builds a structured prompt from the failure logs (`.github/scripts/lib/build-prompt.ts`).
3. Launches a Cursor cloud agent via `@cursor/sdk` with `autoCreatePR: true`.
4. The agent reads the logs, edits the source, and opens a fix PR against the failing branch.
5. The fix PR re-triggers the `tests` workflow to verify.

## Key files

- `.github/workflows/cursor-fix.yml` — GitHub Actions trigger + orchestration
- `.github/scripts/cursor-fix.ts` — entrypoint: validate env → fetchCIContext → buildPrompt → runFixAgent
- `.github/scripts/lib/fetch-logs.ts` — reads `/tmp/failure_logs.txt` + env vars → `CIContext`
- `.github/scripts/lib/build-prompt.ts` — builds the agent prompt from `CIContext`
- `.github/scripts/lib/run-agent.ts` — Cursor SDK invocation, cloud runtime, error handling

## Key design decisions

- **Cloud runtime, not local** — the cloud agent has no Rust toolchain; it reasons from
  logs only. The next CI run on the fix PR validates the change. Use local runtime only if
  you can provision a runner with Rust + Docker.
- **`autoCreatePR: true`** — Cursor opens the fix PR directly; no shell `gh` call needed.
- **`skipReviewerRequest: true`** — suppresses review requests in CI; keeps notifications quiet.
- **Bun** — native TypeScript, fast installs via `bun.lock`, no build step.
- **Log truncation** — `fetch-logs.ts` caps at 8 000 chars to stay within prompt limits.

## Required GitHub secret

- `CURSOR_API_KEY` — Cursor API key from [cursor.com/dashboard/integrations](https://cursor.com/dashboard/integrations). The account must have GitHub access granted so the cloud agent can push and open PRs.

## Extending this pattern

- Change which CI workflow triggers the fix → `on.workflow_run.workflows` in `cursor-fix.yml`
- Tune the agent prompt → `lib/build-prompt.ts`
- Change log capture (lines, format) → `lib/fetch-logs.ts`
- Switch agent model or runtime options → `lib/run-agent.ts`
- Add Slack/notification on fix PR opened → new `lib/notify.ts`, call from `cursor-fix.ts`
- Add retry logic → wrap `runFixAgent` in `cursor-fix.ts`

## Limitations

- The cloud agent cannot run `cargo test` or `docker buildx bake test` to verify its fix
  before opening the PR — verification happens on the fix PR's CI run.
- If the failure requires context beyond the log output (e.g. runtime data, env-specific
  secrets), the agent may not have enough signal to produce a correct fix.
- One fix attempt per failure event; no automatic re-trigger if the fix PR also fails.
