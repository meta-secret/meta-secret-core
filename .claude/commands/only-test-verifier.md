---
description: Run test-verifier only — Agent mode (Bash/tests), formatted report, next-step hints.
---

# Only test verifier

Arguments: optional scope (crate, test filter). Example: `/only-test-verifier -p meta-secret-core`

Delegate to subagent **test-verifier** with input: `$ARGUMENTS`

**Default coverage:** when `$ARGUMENTS` is empty or does not narrow Rust crates, the verifier should run (1) the default **`cargo test`** bundle including **`meta-secret-tests`** and **`meta-secret-wasm`** (see **[`.claude/agents/test-verifier.md`](../agents/test-verifier.md)** — “Default scope”), and (2) **web-cli** npm: **`npm run test:unit`** and **`npm run test:e2e:ci`** from **`meta-secret/web-cli/ui`** (WASM **`pkg/`** may be required first). Server, DB, and mobile FFI crates are **skipped** by default for Cargo. If the user passes only **`-p …`**, run that Cargo subset and **skip** web-cli unless they ask for it. Optional full workspace: plain **`cargo test`** from **`meta-secret/`**.

## Session mode

- **Use Agent mode** (or any mode that allows **Bash**) — running **`cargo test`**, **`npm`** in **`web-cli/ui`**, and **`docker buildx bake test`** requires command execution, not Plan-only.
- This phase is **verification**: run tests and report pass/fail; it is **not** the same as writing tests (**test-author**).

## Presentation (required)

When presenting results:

1. Lead with a **short summary** (emoji ok: pass/fail overall).
2. Put **command lines** and **long log excerpts** in **fenced code blocks**; keep emoji mainly in the summary, not inside raw compiler output.
3. **Bold** failing crate/test name; bullet **actionable** next checks if red.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-debug-rca` | `/only-debug-rca` | If failures need root-cause analysis |
| `/core-only-planner` | `/only-planner` | If failures imply a design change |
| `/core-only-implementer` | `/only-implementer` | If a small code fix is enough |
| `/core-only-release-notes` | `/only-release-notes` | If tests are green and you want MR text |

Typical next step if **red**: **`/core-only-debug-rca`** or **`/core-only-implementer`**. If **green**: **`/core-only-reviewer`** or **`/core-only-release-notes`**.

See [WORKFLOW.md](../WORKFLOW.md).
