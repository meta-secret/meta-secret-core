---
name: cli-guardian
description: Guard architecture, style, and security in cli/meta-cli changes.
---

# CLI Guardian

Use this skill for edits under `meta-secret/cli/` and `meta-secret/meta-cli/`.

## Focus

1. Keep CLI as an adapter over core APIs.
2. Keep command behavior predictable and script-friendly.
3. Keep output safe and non-leaking.

## Architecture rules

- CLI parses args, orchestrates flows, and renders output.
- Business logic and crypto decisions stay in `meta-secret/core`.
- Template/render code must not duplicate domain rules from core.

## Code style rules

- Prefer explicit command modules per use case.
- Keep machine-readable output stable (JSON/YAML templates).
- Use consistent exit codes and typed error mapping.

## Security rules

- Never print raw secret material by default.
- Redact or truncate sensitive identifiers in errors/logs.
- Validate user-provided file paths and input formats before processing.

## Verify before finish

- `cargo test -p meta-secret-cli -p meta-cli`
- Run a representative command manually when CLI output format changed.
