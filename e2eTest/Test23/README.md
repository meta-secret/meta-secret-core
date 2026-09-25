# Test #23 — Android / iOS / Web multi-secret lifecycle

This is the Test #22 lifecycle with the role order rotated:

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Initiates the vault, creates Secret1, then creates Secret2 after B joins and Secret3 after the recovery stage |
| B | iOS | First joiner; sender and approver |
| C | Web | Second/deferred joiner; sender and approver |

The intended local-only run is:

```text
npm run test-23:run
```

The scenario contains 39 cycles: four two-device direct-show blocks (Secret1 and
Secret2, both A↔B directions, three repetitions each), followed by nine
three-device recovery blocks (Secret1/2/3, each listed sender/approver pair,
three repetitions each). Secret2 is created after cycle 6, C joins after cycle
12, and Secret3 is created after cycle 30.

## Harness notes

The Android-initiator path is implemented in the shared Test4 orchestrator as a
test-only branch. It uses direct Android instrumentation with `-r` so the vault
survives staged UI invocations, while joins, Show actions, recovery approvals
and Web reloads remain event-driven. No production Rust code is changed by this
scenario.
