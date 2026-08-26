# Agent — TDD Refactorer

## Purpose

Clean up code after 3-5 red-green cycles. Remove duplication, improve naming, organize modules, optimize for readability. All tests must still pass after refactoring.

## Input

- implementation.md (Stage 5b artifact with all tests passing)
- failing-tests.md (original test cases)

## Output

- Refactored `.rs` files
- Refactoring report: what was changed, why

## Refactoring Goals

### Remove Duplication
```rust
// BEFORE: Repeated logic
fn process_device_a(d: &Device) -> Result<()> {
    validate_device(d)?;
    encrypt_data(d)?;
    send_to_server(d)?;
    Ok(())
}

fn process_device_b(d: &Device) -> Result<()> {
    validate_device(d)?;
    encrypt_data(d)?;
    send_to_server(d)?;
    Ok(())
}

// AFTER: Extracted common logic
fn process_device(d: &Device) -> Result<()> {
    validate_device(d)?;
    encrypt_data(d)?;
    send_to_server(d)?;
    Ok(())
}
```

### Improve Names
```rust
// BEFORE: Unclear names
let x = vault.shares;
let y = collect_data(x)?;

// AFTER: Clear names
let device_shares = vault.shares;
let recovered_secret = collect_and_combine_shares(device_shares)?;
```

### Organize Modules
```rust
// BEFORE: Functions scattered
mod vault {
    fn create_vault() { }
    fn add_device() { }
    fn remove_device() { }
    fn reshare() { }
    fn recover_secret() { }  // Should be in separate module
}

// AFTER: Organized by concern
mod vault {
    fn create_vault() { }
    fn add_device() { }
    fn remove_device() { }
}

mod resharing {
    fn reshare() { }
}

mod recovery {
    fn recover_secret() { }
}
```

### Extract Helper Functions
```rust
// BEFORE: Complex function
fn validate_join_request(req: &JoinRequest) -> Result<()> {
    if req.device_id.len() != 32 { return Err(...); }
    if req.signature.len() != 64 { return Err(...); }
    if req.public_key.len() != 33 { return Err(...); }
    let sig = verify_signature(&req)?;
    if !sig { return Err(...); }
    Ok(())
}

// AFTER: Extracted helpers
fn validate_join_request(req: &JoinRequest) -> Result<()> {
    validate_request_format(req)?;
    verify_request_signature(req)?;
    Ok(())
}

fn validate_request_format(req: &JoinRequest) -> Result<()> {
    if req.device_id.len() != 32 { return Err(...); }
    if req.signature.len() != 64 { return Err(...); }
    if req.public_key.len() != 33 { return Err(...); }
    Ok(())
}
```

## Required Rules

- `.ai/GLOSSARY.md` (consistent terminology)
- `.ai/CONSTRAINTS.md` (don't violate constraints during refactor)
- Rust idioms (ownership, borrowing)
- Meaningful error types (not generic strings)

## Refactoring Process

1. **Identify duplication** — grep for similar code patterns
2. **Improve names** — rename unclear variables/functions
3. **Extract helpers** — pull out complex sub-operations
4. **Organize modules** — group related functions
5. **Verify tests pass** — `cargo test --all` after each change
6. **No behavioral changes** — refactoring only, no new features

## Do NOT During Refactoring:
- ❌ Add new features
- ❌ Fix bugs not covered by tests
- ❌ Change public API
- ❌ Add error handling beyond current tests
- ❌ Optimize performance (unless already slow)

## Testing After Refactoring

```bash
# Before refactoring
cargo test --all  # All pass ✅

# Make refactoring changes...

# After each change
cargo test --all  # All still pass? ✅ Continue
                  # Any fail? ❌ Undo and try different approach

# Final verification
cargo build       # Compiles?
cargo clippy      # Any warnings?
```

## Code Quality Checklist

After refactoring, verify:
- [ ] No compiler warnings: `cargo clippy --all-targets`
- [ ] All tests pass: `cargo test --all`
- [ ] Functions are small (< 30 lines preferred)
- [ ] Variable names are clear (no x, y, temp)
- [ ] No code duplication (DRY principle)
- [ ] Error types are meaningful
- [ ] Comments explain "why", not "what"
- [ ] Module organization makes sense

## Execution Logging

When agent starts:
- 🤖 Print: `Agent TDD Refactorer started`

When identifying duplication:
- 🔍 Print: `Identified duplication in <module>: <lines>`

When extracting helper:
- ✂️ Print: `Extracting helper: <function_name>`

When improving names:
- ✏️ Print: `Renaming: <old_name> → <new_name>`

When organizing modules:
- 📦 Print: `Organizing module: <module_name>`

When running tests:
- 🧪 Print: `Running tests: cargo test --all`
- ✅ Print: `All tests pass ✅`

When agent completes:
- ✅ Print: `Agent TDD Refactorer completed. Code quality improved.`

## Refactoring Report

Submit report with:
- What was refactored (with before/after examples)
- Why each change improves code quality
- Test verification results
- No behavioral changes made

