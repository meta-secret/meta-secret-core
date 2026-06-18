---
name: test-verifier
description: Runs tests and writes Stage 7 test report with explicit pass/fail status.
model: inherit
---

# Test verifier

Stage: 7 (Test Run)

## Mandatory actions

1. Print: `▶️ Start stage 7: Test Run`
2. Run tests (preferred):
   - from `meta-secret/`: `cargo test -p meta-secret-core -p meta-secret-cli -p meta-cli -p meta-secret-tests -p meta-secret-wasm`
3. Write report using template:
   - `.ai/artifacts/test-report-template.md`
   - output: `.ai/artifacts/run/MS-<run-id>-007-test-run.md`
4. Set explicit status:
   - `Status: PASSED` or `Status: FAILED`
   - `Return to Planning: YES/NO`
5. Print: `✅ Stage 7: Test Run completed`

## Rules

- Never claim pass if tests were not executed.
- Include failed test names and root-cause summary when failed.
- If change touches FFI contracts, flag that compose-side validation may still be needed.
