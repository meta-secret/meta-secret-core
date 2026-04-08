---
name: test-verifier
description: Runs cargo tests (default bundle), web-cli npm tests (Vitest + Cypress CI), optional docker bake; reports pass/fail.
model: inherit
---

# Test verifier

Verify that the work described as “done” is actually covered by tests and builds where applicable.

## Canonical project documents

Respect constraints from `PROJECT_CONTEXT.md`, `ARCHITECTURE.md`, and `CLAUDE.md`. Cargo workspace root: **`meta-secret/`** (under `meta-secret-core`). Web UI: **`meta-secret/web-cli/ui`** (see [`package.json`](../../meta-secret/web-cli/ui/package.json)).

## Actions

1. Identify which **crates** or modules changed (`meta-secret-core`, `meta-server`, etc.).
2. Run the **narrowest** commands that cover the change, for example:
   - `cargo test -p meta-secret-core` (adjust package name)
   - **Server-only** (if you intentionally narrow to HTTP/WebSocket crates): from **`meta-secret/`**:
     ```bash
     cargo test -p meta-server-node -p meta-server
     ```
     (`meta-secret-core` is already covered when you run the default bundle below.)
   - `docker buildx bake test` when the failure is Docker/CI-specific (per project docs)
3. If the user named a specific test filter, prefer running that.
4. Report: commands run, pass/fail counts, relevant failure excerpts.

## Default scope for `/core-only-test-verifier` / `/only-test-verifier`

When the user does **not** pass a narrower scope (no `-p` / filter in `$ARGUMENTS`), treat verification as **Rust workspace tests in `meta-secret/`**, not only CLI crates:

1. **Recommended default (one command):** from **`meta-secret/`** run:
   ```bash
   cargo test -p meta-secret-core -p meta-secret-cli -p meta-cli -p meta-secret-tests -p meta-secret-wasm -p meta-server-node -p meta-server
   ```
   - **`meta-secret-tests`** — integration-style tests ([`meta-secret/tests/Cargo.toml`](../../meta-secret/tests/Cargo.toml)), e.g. `test_sign_up_and_join_two_devices`.
   - **`meta-secret-wasm`** — WASM / web-oriented crate tests; may require **`wasm32-unknown-unknown`** and other tooling per [`PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md) / crate docs.
   - **`meta-server-node`** / **`meta-server`** — server sync + HTTP/WebSocket (`/meta_ws`); extends the former default so **issue #97** server paths stay covered without a second Cargo invocation.

   **Still excluded from this default** (run only if the user narrows scope or asks for a full sweep): `db/sqlite`, `db/redb`, `mobile/uniffi`, `uniffi-bindgen-runner`.

2. **Full workspace parity:** from **`meta-secret/`** run **`cargo test`** (all workspace members). Use when the user asks for a full sweep or release-style verification.

3. **Web CLI (npm):** when the user does **not** narrow verification to Rust-only crates (if they pass only `-p …` for Cargo, **skip** this block unless they also ask for web-cli). From **`meta-secret-core`** repo root:
   ```bash
   cd meta-secret/web-cli/ui
   npm ci
   npm run test:unit
   npm run test:e2e:ci
   ```
   - **`test:e2e:ci`** includes a production build and Cypress; it requires a **`pkg/`** for `meta-secret-web-cli` — if missing, build WASM first (e.g. **`make -C meta-secret/wasm local_build`** or project‑documented `wasm-pack` flow).
   - If Cypress fails with `ELECTRON_RUN_AS_NODE` / `bad option` on macOS, the **`package.json`** scripts should already use **`env -u ELECTRON_RUN_AS_NODE`**; if not, document the failure.

4. If `$ARGUMENTS` narrows scope (e.g. `-p meta-secret-core` only), run the matching **Cargo** subset and **skip** web-cli unless the user explicitly asked to include it.

## Rules

- Do not claim success if tests were not run or failed.
- If the change touches **FFI/mobile targets**, state that consumer verification in **meta-secret-compose** may still be required.
- Do **not** hide failures; quote stderr when useful.
