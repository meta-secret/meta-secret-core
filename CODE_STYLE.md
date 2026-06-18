# CODE_STYLE.md

## Rust

- **Edition:** Workspace uses modern Rust (see `Cargo.toml`); follow `cargo fmt` for formatting.
- **Naming:** Follow Rust API guidelines (`snake_case` for functions/modules, `PascalCase` for types).
- **Errors:** Prefer typed errors (`thiserror` / explicit enums) at boundaries; avoid stringly-typed failures in public APIs.
- **Async:** Use the stack already present in each crate (`async-std` / `tokio` as applicable); do not mix runtimes in one module without a strong reason.
- **Imports:** Avoid fully qualified `crate::...` paths in function bodies/type positions when `use` imports can be used. Prefer importing modules/types at the top of the file and referencing short names in code.
- **Test placement:** Keep small unit tests close to the module they validate (same file) only when they test local/private logic. Put long multi-step scenarios (sync/join/recovery flows) in `meta-secret/tests/...` integration tests.
- **Test block order:** If tests are in the same source file (`#[cfg(test)] mod tests`), keep that module at the end of the file, after all production code.

## Tests

- Prefer deterministic tests; mock time and IO where needed.
- Name tests after behavior (`rejects_invalid_share`), not only after function names.

## AI discipline

- Minimal diffs; no drive-by reformat of unrelated files.
- Do not add verbose comments for obvious code; add comments where invariants are non-obvious (especially crypto).

## Tools

- Run `cargo fmt` and `cargo clippy` where applicable before submitting (or let CI catch issues).
