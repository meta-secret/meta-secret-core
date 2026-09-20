# CI Monitor Rule

- Monitor only the `CI` workflow unless another workflow is explicitly added.
- Resolve a PR by exact `head_sha`; launch Cursor only when its head repository
  equals the base repository.
- Never send sensitive, permission, mixed, unknown, or deployment failures to
  an external agent.
- Retry transient infrastructure failures at most twice (`run_attempt < 3`).
- Upload only bounded, redacted `ci-monitor-<run>-<attempt>` artifacts; do not
  create implementation-stage artifacts for monitor runs.
- Keep `actions:write`, `pull-requests:write`, and `CURSOR_API_KEY` scoped to
  the jobs that need them.

`.github/scripts/lib/ci-monitor.ts` is the single classifier and retry policy.
