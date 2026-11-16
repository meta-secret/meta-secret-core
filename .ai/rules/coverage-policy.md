# Coverage policy

Overall percentage targets are advisory, including the 80% goal. A low percentage alone does not block a change. The coverage stage records the actual measured scope, percentage, uncovered behavior and tooling limitations. Required behavioral tests must pass; measurement errors are not successful coverage and require resolution or an explicit recorded non-applicability/waiver. Prefer high coverage for changed security-sensitive behavior without inventing a universal numeric release threshold.

For Rust changes use a supported configured Rust coverage tool and report scope; for Web-only changes use applicable Web reporting. Do not run tarpaulin or rebuild native libraries for a Vue/CSS-only change. Do not silently introduce a new runner or dependency only to print a number. See ui-migration-profile.md for the approved UI migration applicability matrix.
