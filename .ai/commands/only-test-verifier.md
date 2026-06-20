---
description: Run test-verifier only — Agent mode (Bash/tests), formatted report, next-step hints.
---

# Only test verifier

Arguments: optional scope (crate, test filter). Example: `/only-test-verifier -p meta-secret-core`

Delegate to subagent **test-verifier** with input: `$ARGUMENTS`

**Default coverage:** when `$ARGUMENTS` is empty, run **`task test`** from **repository root** (CI parity — see [`.ai/skills/build-via-task/SKILL.md`](../../skills/build-via-task/SKILL.md) and [`.ai/agents/test-verifier.md`](../agents/test-verifier.md)). Optional narrow local check: `cargo test -p …` from `meta-secret/` when `$ARGUMENTS` narrows crates. For web-cli-only changes, add **`npm run test:unit`** / **`npm run test:e2e:ci`** in **`meta-secret/web-cli/ui`** (WASM **`pkg/`** may require **`task wasm-local`** first).

## Session mode

- **Use Agent mode** (or any mode that allows **Bash**) — running **`task test`**, **`npm`** in **`web-cli/ui`**, and other **`task`** build targets requires command execution, not Plan-only. Read [`.ai/skills/build-via-task/SKILL.md`](../../skills/build-via-task/SKILL.md) first.
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
