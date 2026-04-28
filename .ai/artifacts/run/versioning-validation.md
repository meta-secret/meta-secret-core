# Versioning Gate Validation

## Scope

Validation target:

- `.ai/scripts/verify-versioning.sh`
- `.github/workflows/pr-versioning.yml`

## Scenario 1: patch without version bump (must fail)

- Input:
  - `BASE_SHA=HEAD`
  - `HEAD_SHA=HEAD`
  - `VERSION_DECISION_FILE=.ai/artifacts/run/version-decision.json`
- Expected: fail (`target_version_files` points to a version file that was not changed in diff)
- Actual: fail with
  - `ERROR: code changed, but version file was not updated. Update meta-secret/VERSION.`

## Scenario 2: minor with valid bump (must pass)

- Expected behavior:
  - `bump_type=minor`
  - `meta-secret/VERSION` is changed in PR diff
  - unified version satisfies minor rule:
    - major unchanged
    - minor increased
- Verifier status:
  - supported and enforced by `check_semver_increment()` and target/diff consistency checks.
  - run should return `Versioning check passed.` when inputs satisfy the rules.
