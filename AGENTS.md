# AGENTS.md

This file guides Codex (Codex.ai/code) in **meta-secret-core**. **Canonical detail** lives in the linked documents at this repository root.

## Project documents (read these)

| Document | Contents |
|---|---|
| [WORKFLOW.md](WORKFLOW.md) | Agent phases, GitHub/manual entry, approval gates, subagents |
| [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) | Workspace layout, crates, build/test commands, link to mobile consumer |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crates, crypto boundary, server vs client, FFI/UniFFI |
| [SECURITY.md](SECURITY.md) | Keys, logging, crypto handling, operational hygiene |
| [CODE_STYLE.md](CODE_STYLE.md) | Rust style, tests, AI discipline |

## Non-negotiables (duplicate here for visibility)

- **Cryptography:** Treat crypto and protocol code as high-risk; minimal diffs; no speculative algorithm changes.
- **FFI / UniFFI:** Mobile-facing API changes affect `meta-secret-compose`; coordinate contract changes and version artifacts.
- **Scope:** This repository owns **Rust** (core library, CLI, server, WASM, mobile Rust targets). It does **not** own Kotlin/Swift app UI—that lives in the compose repo.

## Priorities

1. Restore `cargo test` / CI-equivalent with minimal changes.
2. Preserve crate boundaries and public API stability where required.
3. Prefer local fixes over broad refactors.
4. State uncertainty explicitly.

## Forbidden

- Rewrite unrelated crates or “clean up” without need.
- Bump dependency versions unless the error clearly implicates them.
- Log secrets, key material, or raw shares.

## Default repair workflow

1. Run the narrowest **`task`** or **`cargo`** target that reproduces the issue (see [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) and [`.ai/skills/build-via-task/SKILL.md`](.ai/skills/build-via-task/SKILL.md)).
2. Classify the error (compile, test, runtime, infra).
3. Propose a minimal fix plan.
4. Wait for user confirmation when appropriate.
5. Apply the smallest fix; re-verify.

## AI workflow

Follow [WORKFLOW.md](WORKFLOW.md). 

**Unified AI structure:** All AI automation lives in [`.ai/`](.ai/) — **single source of truth** for Codex, Cursor, and OpenAI Codex CLI.

- **Agents:** [`.ai/agents/`](.ai/agents/)
- **Commands:** [`.ai/commands/`](.ai/commands/) (slash commands for Codex + Codex CLI)
- **Skills:** [`.ai/skills/`](.ai/skills/) (reusable workflows)
- **Rules:** [`.ai/rules/`](.ai/rules/) (Cursor + Codex CLI)

IDE entry files in `.Codex/`, `.cursor/`, and `.codex/` bootstrap orchestration and point to `.ai/` as canonical source:
- `.Codex/ORCHESTRATE.md`
- `.cursor/WORKFLOW.md`
- `.codex/ORCHESTRATE.md`

👉 **See [`.ai/ARCHITECTURE.md`](.ai/ARCHITECTURE.md)** for complete AI structure and IDE integration details.

**Agent output:** When this repo sits under the MetaSecret parent workspace, follow [Agent output conventions](../AGENTS.md#agent-output-conventions) in the root `AGENTS.md`. Otherwise use the same norms (emojis in replies; `##`/`###` headings, **bold**, blockquotes; optional HTML color where the UI supports it).

## IDE Support

| IDE | Support | Where |
|-----|---------|-------|
| **Codex** | Workflow bootstrap | Via `.Codex/INDEX.md` + `.Codex/ORCHESTRATE.md` |
| **Cursor** | Workflow bootstrap + entry rule | Via `.cursor/WORKFLOW.md` + `.cursor/rules/00-entry.mdc` |
| **OpenAI Codex CLI** | Workflow bootstrap | Via `.codex/INDEX.md` + `.codex/ORCHESTRATE.md` |

Rules under [`.ai/rules/`](.ai/rules/) remain the canonical source for policy and stage behavior.

## Cursor Cloud specific instructions

Durable, non-obvious notes for working in the Cursor Cloud VM. Tooling already provisioned in the VM snapshot (no need to reinstall): Rust `1.96.0` (pinned by `rust-toolchain.toml`), the `wasm32-unknown-unknown` target, `wasm-pack`, `bun` (at `~/.bun/bin`), and the `diesel` CLI. `task`/Docker are **not** installed — the `task` targets wrap `docker buildx` for CI parity and do not run here, so iterate locally with `cargo`/`bun` instead.

### Rust workspace (root is `meta-secret/`, not the repo root)
- **Always pass `--target x86_64-unknown-linux-gnu`** to `cargo build/test/clippy`. `.cargo/config.toml` sets `rustflags = ["-C", "target-feature=+crt-static"]` globally; with no explicit `--target`, cargo applies it to host proc-macros and the build fails with `cannot produce proc-macro ... the target ... does not support these crate types`. An explicit `--target` scopes the rustflags to the target only (this is what the Dockerfile does).
- Build: `cargo build --target x86_64-unknown-linux-gnu` (run from `meta-secret/`).
- Test: `cargo test --target x86_64-unknown-linux-gnu`.
- Lint: `cargo clippy --target x86_64-unknown-linux-gnu` (warnings only on a clean tree).

### meta-server (HTTP sync server, port 3000)
- The binary requires a **migrated** SQLite DB named `meta-secret.db` in its working directory; `web-server/src/main.rs` does **not** run migrations. Without it the HTTP listener starts but the background sync task panics: `no such table: db_commit_log`.
- Create the DB once (mirrors `meta-secret/Dockerfile`): from `meta-secret/db/sqlite`, run `DATABASE_URL=<run-dir>/meta-secret.db diesel migration run`.
- Run: `cargo run -p meta-server --target x86_64-unknown-linux-gnu` from the run dir (auto-creates `master_key.json` + device credentials in `db_commit_log`). Endpoints: `GET /hi`, `POST /meta_request`.

### CLI split/recover (offline core functionality)
- `meta-secret-cli` reads `config.yaml` (`number_of_shares` / `threshold`) from CWD and writes/reads shares under `secrets/`. `split --secret <s>` creates `secrets/shared-secret-{n}.json` + `.png`; `restore --from json|qr` recovers from any `threshold` shares.

### Web UI (`meta-secret/web-cli/ui`, Vite dev on :5173)
- Consumes the WASM core via the `file:./pkg` dependency, so build the WASM pkg **before** `bun install`: `cd meta-secret/wasm && wasm-pack build --target web`, then copy per `wasm/Makefile` `local_build` (`cp -r pkg ../web-cli/ui/ && cp pkg.package.json ../web-cli/ui/pkg/package.json`). `pkg/` and `node_modules/` are gitignored and persist in the snapshot.
- Install/run: `bun install`, then `bun run dev --host`. Lint: `bun run lint:check` (eslint). Note: a newer `bun` rewrites `bun.lock` (adds `configVersion`) — don't commit that incidental change.
- The app gates the whole UI behind a WebAuthn passkey modal (`PasskeyAuth.vue`, shown whenever not authenticated). In headless/cloud Chrome (no biometrics), enable a DevTools **WebAuthn virtual authenticator** first (Cmd Menu → "Show WebAuthn" → enable virtual authenticator env; ctap2 / internal / resident keys + user verification), then click "Create Passkey". The `/tools/split` and `/tools/recover` pages run fully offline via WASM (split renders QR-code shares; recover uploads QR images).
