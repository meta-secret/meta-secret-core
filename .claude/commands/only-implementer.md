---
description: Run code-implementer only — Agent mode, formatted summary, next-step hints.
---

# Only implementer

Arguments: approved plan text or path context. Example: `/only-implementer <approved plan>`

Delegate to subagent **code-implementer** with input: `$ARGUMENTS`

## Session mode

- **Use Agent mode** — implementation requires **Write** / **Edit** on source files.
- **Scope:** Implement only what the user has **already approved** in a written plan (or explicit narrow instruction).

## Presentation (required)

When reporting results to the user:

1. Use **Markdown** with **emoji section headers** (examples: package for crates touched, wrench for key edits, test tube if tests were added inline).
2. **Bold** file paths and public API changes; use bullet lists for behavioral changes.
3. Summarize **what changed** and **what was left out** if scope was trimmed.

## Next steps — pick a command

- If workspace root is **MetaSecret**, use **`/core-only-*`**; if only **meta-secret-core**, use **`/only-*`**.

| Slash (MetaSecret) | Slash (repo only) | What it does |
|--------------------|-------------------|--------------|
| `/core-only-test-author` | `/only-test-author` | Add or extend tests for the change |
| `/core-only-test-verifier` | `/only-test-verifier` | Run tests and report pass/fail |
| `/core-only-reviewer` | `/only-reviewer` | Review the diff for architecture/style |
| `/core-only-debug-rca` | `/only-debug-rca` | If build or tests fail unexpectedly |

Typical next step: **`/core-only-test-author`** or **`/core-only-test-verifier`** (MetaSecret), or **`/only-test-author`** / **`/only-test-verifier`** (repo root).

See [WORKFLOW.md](../WORKFLOW.md).
