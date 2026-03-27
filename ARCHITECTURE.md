# ARCHITECTURE.md

## High-level

Meta Secret core is organized as a **Cargo workspace** of crates. The **`meta-secret-core`** library centralizes cryptography, protocol-facing types, and shared node/client logic. Binaries and adapters (CLI, server, WASM, DB, mobile staticlibs) depend on it and must not duplicate crypto.

## Crate roles (summary)

| Area | Responsibility |
|------|----------------|
| `meta-secret-core` | Crypto primitives usage, secret sharing flows, node/common APIs, shared models |
| `meta-server` / `meta-server-node` | HTTP server, orchestration with core and DB |
| `meta-db-sqlite` (and related) | Persistence adapters—no business rules that belong in core only |
| `wasm` | Browser/WASM entrypoints—thin over core |
| `cli` / `meta-cli` | User-facing CLI |
| `mobile/ios`, `mobile/android` | Static libraries for FFI; keep FFI boundary thin |

## Boundaries

- **Core vs IO:** Keep pure protocol/crypto reasoning in `meta-secret-core` where possible; push filesystem/network/UI-specific code to the appropriate crate.
- **Server vs client:** Server crates orchestrate transport and storage; they must not silently weaken crypto assumptions documented in code and [SECURITY.md](SECURITY.md).
- **FFI:** UniFFI/mobile exports are a **public contract**. Changes require review for backward compatibility and parallel updates in **meta-secret-compose**.

## SOLID (practical)

- Prefer small modules and traits over god-objects.
- Inject dependencies (constructors/config) rather than hidden globals for testability.
- Extend behavior via new types or explicit enums—not unrelated `cfg` sprawl without cause.

## Testing

- Unit tests live beside code or under crate `tests/` per Rust conventions.
- Integration tests that need several crates belong in the `tests` workspace member or the relevant crate.

## Further reading

- [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) — paths and commands.
- `.claude/skills/architecture-guardian/SKILL.md` — short checklist for agents (align with this file).
