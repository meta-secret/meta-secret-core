# MetaSecret artifact versions

Manual catalog. When you release or bump a version: update the **native** file first (`Cargo.toml`, `package.json`), then update this file in the **same commit**.

Last updated: 2026-04-28

## Tracked artifacts

| Artifact | Version | Native version location |
|----------|---------|-------------------------|
| Core (`meta-secret-core`) | 2.0.0 | `meta-secret/core/Cargo.toml` |
| Web UI (`meta-secret-ui-vue`) | 2.0.0 | `meta-secret/web-cli/ui/package.json` |
| WASM (`meta-secret-wasm`) | 2.0.0 | `meta-secret/wasm/Cargo.toml` |
| Server (`meta-server`) | 2.0.0 | `meta-secret/meta-server/web-server/Cargo.toml` |
| CLI (`meta-cli`) | 2.0.0 | `meta-secret/meta-cli/Cargo.toml` |
| CLI minimal (`meta-secret-cli`) | 2.0.0 | `meta-secret/cli/Cargo.toml` |
| DB SQLite (`meta-db-sqlite`) | 2.0.0 | `meta-secret/db/sqlite/Cargo.toml` |
| DB redb (`meta-db-redb`) | 2.0.0 | `meta-secret/db/redb/Cargo.toml` |

## Client “Server” line

Web and CLI show **Server** using compile-time `meta-server-node` (`meta-secret/meta-server/server-node/Cargo.toml`). The HTTP binary crate `meta-server` may differ; the user-facing label **Server** refers to this shared stack version.

## Mobile app (separate repository)

End-user **mobile app** versioning is owned by the **meta-secret-compose** repository (Android / iOS). It is not a crate in this workspace; see that repo for app store / bundle versions.

## Not tracked here (internal / tooling)

- `uniffi-bindgen-runner`, `meta-secret-tests`, other workspace-only crates unless promoted to released artifacts.

## Manual release workflow

1. Decide which artifact(s) change in this release.
2. Bump `version` in the corresponding native file(s).
3. Update the table above and `Last updated`.
4. Run `cargo build --workspace` / `cargo test --workspace`; for Web UI run `npm run build` under `meta-secret/web-cli/ui`.
5. Commit version changes and this file together.
