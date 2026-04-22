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
- Add comments only for invariants and non-obvious constraints.

## Security rules

- Never log secrets, key bytes, shares, or decrypted payloads.
- Crypto changes require minimal diff plus focused tests.
- Do not weaken validation paths for convenience.

## Verify before finish

- `cargo test -p meta-secret-core`
- If shared contracts changed, run affected workspace package tests too.
