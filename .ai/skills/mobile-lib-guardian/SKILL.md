---
name: mobile-lib-guardian
description: Guard architecture, style, and security in mobile/uniffi library changes.
---

# Mobile Lib Guardian

Use this skill for edits under `meta-secret/mobile/` and UniFFI bridge surfaces.

## Focus

1. Keep Rust mobile library ABI/API stable.
2. Keep FFI layer thin over core logic.
3. Prevent leaks across FFI boundaries.

## Architecture rules

- `mobile/uniffi` defines exported contract and should delegate to core/common modules.
- `mobile/common` may adapt data and platform helpers, but not reimplement core domain logic.
- Any `.udl` surface change must be treated as a versioned contract change.

## Code style rules

- Keep FFI types explicit and serialization stable.
- Prefer additive API evolution over breaking changes.
- Keep bridge code small and testable.

## Security rules

- Never expose raw key material or decrypted secrets via logs or debug helpers.
- Validate all FFI input size/format at boundary entry.
- Treat panic propagation across FFI as a bug: return controlled errors.

## Verify before finish

- `cargo test -p metasecret_mobile`
- `cargo run -p uniffi-bindgen-runner --bin uniffi-bindgen` when interface changed.
