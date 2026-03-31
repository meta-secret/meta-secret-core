# PROJECT_CONTEXT.md

## What this repository is

**meta-secret-core** is the **Rust workspace** for Meta Secret: cryptographic core, sync/server components, WASM, CLI, and mobile FFI/static library targets. The **Kotlin Multiplatform app** that consumes the library via FFI lives in a separate repo (**meta-secret-compose**).

## Workspace root (important)

Rust **Cargo workspace** root: **`meta-secret/`** (not the git repository root).

| Path | Role |
|------|------|
| `meta-secret/Cargo.toml` | Workspace manifest (`members`: core, db, server, wasm, cli, mobile, tests, …) |
| `meta-secret/core/` | `meta-secret-core` library—crypto, node logic, shared types |
| `meta-secret/meta-server/` | HTTP server and server-node |
| `meta-secret/wasm/` | WASM build |
| `meta-secret/cli/`, `meta-secret/meta-cli/` | CLI binaries |
| `meta-secret/mobile/uniffi` | UniFFI crate (library `metasecret_mobile`); Kotlin/Swift via `cargo run -p uniffi-bindgen-runner --bin uniffi-bindgen` |
| `meta-secret/mobile/uniffi-bindgen-runner` | Workspace package exposing the `uniffi-bindgen` binary |
| `meta-secret/db/sqlite`, `meta-secret/db/redb` | Database adapters |
| `infra/` | Infrastructure (e.g. Docker/K8s-related assets—see repo layout) |

## Builds and tests (typical)

From **`meta-secret/`** (adjust package flags as needed):

```bash
cargo test
cargo test -p meta-secret-core
cargo build --release -p meta-server
```

Project-wide Docker builds/tests (see root [README.md](README.md) and [`.cursor/rules/build/`](.cursor/rules/build/)):

```bash
docker buildx bake test
```

Mobile: `meta-secret/mobile/scripts/build-mobile.sh [ios|android|all]` (wraps `build-ios.sh` / `build-android.sh`).

Use the **narrowest** command that proves the fix.

## Consumer repo

- **meta-secret-compose:** KMM app; consumes artifacts produced here. Breaking FFI or UniFFI surface requires coordinated releases and compose updates.

## Documentation map

- [ARCHITECTURE.md](ARCHITECTURE.md) — structure and boundaries.
- [SECURITY.md](SECURITY.md) — secrets and crypto hygiene.
- [CODE_STYLE.md](CODE_STYLE.md) — Rust conventions.
- [WORKFLOW.md](WORKFLOW.md) — AI delivery pipeline.
