---
name: core-guardian
description: Guard architecture, style, and security in meta-secret core crate changes.
---

# Core Guardian

Use this skill for edits under `meta-secret/core/` and adjacent shared Rust models.

## Focus

1. Keep domain and crypto logic in `meta-secret/core`.
2. Keep transport/UI/framework concerns out of core.
3. Preserve deterministic behavior and testability.

## Architecture rules

- Core owns secret split/recovery logic, crypto wrappers, and shared domain types.
- Core must not depend on web UI, mobile UI, or CLI formatting concerns.
- Extend behavior via explicit types and module boundaries, not by adding hidden globals.

## Code style rules

- Follow workspace rustfmt and existing module naming.
- Prefer typed errors and explicit enums at crate boundaries.
- Prefer top-level `use` imports over long fully-qualified `crate::...` paths inside code.
- Keep short unit tests for private/local behavior in the same file, but move long join/sync/recovery scenarios to `meta-secret/tests/...`.
- If tests stay in the same file, keep `#[cfg(test)] mod tests` strictly at the end of the file.
- Add comments only for invariants and non-obvious constraints.

## Security rules

- Never log secrets, key bytes, shares, or decrypted payloads.
- Crypto changes require minimal diff plus focused tests.
- Do not weaken validation paths for convenience.
- Temporary threshold policy: fixed `K=2` for `N>=2`.
- TODO: migrate threshold strategy to `K=N-1` after protocol hardening.

## Verify before finish

- `cargo test -p meta-secret-core`
- If shared contracts changed, run affected workspace package tests too.
