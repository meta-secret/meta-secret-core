# Build via Taskfile

Use this skill **before running any build, test, or Docker verification command** in meta-secret-core.

## Mandatory rule

**Entry point:** `task <target>` from the **repository root**.

**Forbidden** in agent shells (unless you are editing `Taskfile.yml` itself):

- `docker buildx bake …`
- `docker buildx build …`
- `docker build …`

`docker-bake.hcl` and `meta-secret/Dockerfile` define *how* builds work; **Taskfile.yml** is *how you invoke them*.

## Allowed exceptions

| Context | Allowed command |
|---------|-----------------|
| Narrow Rust edit in one crate | `cargo test -p …`, `cargo build -p …` from `meta-secret/` |
| web-cli UI only (no Docker/WASM) | `npm run lint:check`, `npm run build` in `meta-secret/web-cli/ui/` |
| Adding a missing task | Edit `Taskfile.yml` first, then run the new `task` target |

If CI parity matters (Dockerfile, bake, WASM, web dist), **use `task`**, not `cargo`/`npm` alone.

## Task → use case

Run `task -l` from repo root for the full list.

| Goal | Command |
|------|---------|
| CI-equivalent tests | `task test` |
| Warm Rust test cache (CI) | `task warm-cache` |
| Warm WASM dep cache (CI) | `task warm-cache-wasm` |
| Build web dist locally | `task web-local` |
| Build WASM pkg locally | `task wasm-local` |
| Build web Docker image | `task web` |
| Build meta-server image | `task meta-server` |
| Build all default images | `task build` |
| Build + push images | `task push` |
| Regenerate cargo-chef recipe | `task generate-recipe` (after any `Cargo.toml` change) |
| Playwright smoke tests | `PLAYWRIGHT_BASE_URL=… task playwright-test` |
| Run web-ui dev server | `task web-run` |

## Change → verify mapping

Pick the **narrowest** task that covers your edit:

| Files touched | Verify with |
|---------------|-------------|
| `meta-secret/Dockerfile`, `docker-bake.hcl`, `Taskfile.yml`, `recipe.json` | `task web-local` and/or `task wasm-local` and/or `task test` |
| `meta-secret/**/Cargo.toml` (workspace deps) | `task generate-recipe` then `task test` or `task warm-cache` |
| `meta-secret/wasm/**` | `task wasm-local` |
| `meta-secret/web-cli/**` (full stack incl. WASM in Docker) | `task web-local` |
| `meta-secret/web-cli/ui/**` (UI only, no Docker) | `npm run lint:check && npm run build` in `web-cli/ui` |
| Server / core Rust | `task test` or narrow `cargo test -p …` then `task test` before PR |

## Missing task?

Do **not** fall back to raw `docker buildx`.

1. Add a task to root `Taskfile.yml` wrapping the bake target.
2. Document it in `.ai/rules/build/build.mdc` and `.cursor/rules/build/build.mdc`.
3. Run `task <new-target>`.

## Pipeline stages

- **Stage 4 (Build):** run the mapped `task` target(s) from this skill; record exact commands in the build report.
- **Stage 7 (Test Run):** prefer `task test` for CI parity; local `cargo test -p …` is OK for narrow iteration only.

## Pre-command checklist (agents)

Before any shell command that builds or tests:

1. Does it match `docker buildx` or `docker build`? → **Stop.** Use `task` or add a task.
2. Is this Docker/CI parity? → **`task …`**
3. Is it UI-only in `web-cli/ui`? → **`npm run …`** OK
4. Is it a single-crate Rust check? → **`cargo …`** OK, then confirm with `task test` before PR
