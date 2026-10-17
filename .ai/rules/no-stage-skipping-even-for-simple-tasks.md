# Stage completeness and applicability

WORKFLOW.md defines the canonical Core stage sequence. Keep an artifact for every stage and explicit applicability. Read rules/ui-migration-profile.md for client-only work and rules/coverage-policy.md for advisory percentages. Older Rust-only examples do not force cargo/tarpaulin for Web/CSS changes.

Planning and constraint validation precede implementation. Clarification uses approved session decisions and asks only unresolved consequential questions. Meaningful behavioral tests remain required; appearance and motion are verified against approved references rather than trivial implementation-mirroring tests. Security/code review and applicable visual/motion review remain gates, with diagrams only when the relevant architecture changes. Record measured coverage honestly; 80% is advisory.

Use cheap scoped validation before expensive platforms; reuse unchanged built artifacts and run applicable final UI E2E through the existing root skill. Missing artifacts, unreported failures, invented test success and silent boundary changes remain invalid. Reuse explicit session authorization for the exact release action; commit permission does not grant push/PR/merge/deploy.
