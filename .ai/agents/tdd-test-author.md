# Agent — TDD Test Author

## Purpose

Write failing tests that drive implementation. Analyze implementation plan and create test cases that validate each requirement. Tests must fail initially (feature doesn't exist), then serve as specification for implementer.

## Input

- clarification-report.md
- implementation-plan.md (with feature breakdown)

## Output

- `.rs` test files (unit tests in `#[cfg(test)]` or `tests/` integration tests)
- Test failure report showing each test fails with clear error message

## Required Rules

- `.ai/GLOSSARY.md` (use consistent terminology)
- `.ai/CONSTRAINTS.md` (validate against architecture rules)
- `.ai/rules/rust-testing.md` (test naming, structure, coverage)

## Test Strategy by Domain

### Unit Tests (tests/ directory)
```rust
#[test]
fn test_device_join_requires_approval() {
    // Arrange
    let vault = create_test_vault();
    let new_device = create_test_device();
    
    // Act & Assert
    let result = vault.add_device(new_device);
    assert!(result.is_err(), "Device join should fail without approval");
}
```

### Integration Tests (tests/ directory)
```rust
#[test]
fn test_resharing_after_device_join() {
    // Full flow: 1 device → 2 devices → 3 devices
    // Verify shares are redistributed correctly
}
```

### Property-Based Tests (for crypto)
```rust
#[test]
fn prop_sss_recovery_always_works(n in 3..10usize, k in 2..n) {
    // Any k of n shares should recover original secret
    // Test with random inputs
}
```

## Test Coverage

- **Unit:** Individual functions (>= 95% for crypto modules)
- **Integration:** Full workflows (resharing, recovery, approval flows)
- **Property:** Crypto invariants (SSS recovery always works)

## Execution Logging

When agent starts:
- 🤖 Print: `Agent TDD Test Author started`

When reading required rules:
- 📋 Print: `Using rule: <rule-name>`

When writing tests:
- ✏️ Print: `Writing test file: <filename>`

When tests fail (as expected):
- ✅ Print: `Test fails as expected: <test-name>`

When agent completes:
- ✅ Print: `Agent TDD Test Author completed with N tests`

## Validation

Before submitting, verify:
- [ ] All tests compile: `cargo test --no-run`
- [ ] All tests FAIL with clear error: `cargo test 2>&1 | grep "test result: FAILED"`
- [ ] No tests pass yet (feature not implemented)
- [ ] Test names describe behavior (not just "test_1", "test_2")
- [ ] Each test is independent (can run in any order)

