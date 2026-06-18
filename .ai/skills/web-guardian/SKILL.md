---
name: web-guardian
description: Guard architecture, style, and security in web-cli/ui changes.
---

# Web Guardian

Use this skill for edits under `meta-secret/web-cli/ui/` and WASM-facing web integration points.

## Focus

1. Keep UI thin over API/core contracts.
2. Keep component state and routing explicit.
3. Prevent client-side leaks of secret data.

## Architecture rules

- Vue components should orchestrate UI state, not embed crypto/domain logic.
- Shared business rules must live in Rust core/WASM contracts.
- Keep boundaries clear between router, UI components, and API managers.

## Code style rules

- Use Vue Composition API.
- Prefer strongly typed TS interfaces at boundaries.
- Keep component-scoped styles and avoid noisy debug logging.

## Security rules

- Do not persist decrypted secrets to long-lived browser storage.
- Avoid logging sensitive payloads in browser console and telemetry.
- Validate and sanitize all external input before rendering.

## Verify before finish

- `cd meta-secret/web-cli/ui && npm run build`
- Run targeted unit/e2e tests when routes or auth-relevant paths change.
