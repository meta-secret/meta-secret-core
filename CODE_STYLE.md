# CODE_STYLE.md

## Rust

- **Edition:** Workspace uses modern Rust (see `Cargo.toml`); follow `cargo fmt` for formatting.
- **Naming:** Follow Rust API guidelines (`snake_case` for functions/modules, `PascalCase` for types).
- **Errors:** Prefer typed errors (`thiserror` / explicit enums) at boundaries; avoid stringly-typed failures in public APIs.
- **Async:** Use the stack already present in each crate (`async-std` / `tokio` as applicable); do not mix runtimes in one module without a strong reason.

## Tests

- Prefer deterministic tests; mock time and IO where needed.
- Name tests after behavior (`rejects_invalid_share`), not only after function names.

## AI discipline

- Minimal diffs; no drive-by reformat of unrelated files.
- Do not add verbose comments for obvious code; add comments where invariants are non-obvious (especially crypto).

## Tools

- Run `cargo fmt` and `cargo clippy` where applicable before submitting (or let CI catch issues).
