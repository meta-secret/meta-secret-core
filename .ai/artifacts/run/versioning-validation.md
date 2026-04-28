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
  - `ERROR: meta-secret/web-cli/ui/package.json is listed in target_version_files but not changed.`

## Scenario 2: minor with valid bump (must pass)

- Expected behavior:
  - `bump_type=minor`
  - listed target version files are changed in PR diff
  - each target version satisfies minor rule:
    - major unchanged
    - minor increased
- Verifier status:
  - supported and enforced by `check_semver_increment()` and target/diff consistency checks.
  - run should return `Versioning check passed.` when inputs satisfy the rules.
