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

Creation actions are post-cycle barriers: after the closed Show/result signals
for cycle 6, Android creates Secret2 and waits for its creation marker/result
before cycle 7. After the closed recovery/result signals for cycle 30, it
creates Secret3 and waits for confirmation before cycle 31. The reusable
`shared/post-cycle-actions.mjs` helper prevents duplicate boundary execution;
its regression test checks the full JSON plan (39 cycles, 27 recoveries).

The shared Android recovery receiver gate records bounded test-only evidence
at readiness, changed matching pending state and badge timeout: exact pending
and newly observed claim IDs, local secret-name equality counts/hashes, composed
row hashes, selected tabs and row/badge/open/alert/processing visibility/bounds.
It reads cached StateFlows and emits no raw names, values or state payloads.
The original badge assertion/deadline is retained. No viewport fix is assumed
without evidence; these diagnostics distinguish mapping/rendering from an
uncomposed or offscreen row before a production change is proposed.

Every iOS step now binds its cycle-selected Secret consistently in both the
host XCTest step file and launch environment. `shared/ios-step-secret.mjs`
rejects conflicting explicit/environment names or values before actor launch;
missing per-step selection does not inject Secret1 into the environment.
Values are resolved from the selected scenario Secret, and binding diagnostics
emit only a selector hash. This guard covers the shared multi-secret actor
factories, including creator/Show/redistribution roles, rather than Test #23
alone. `shared/ios-step-secret.test.mjs` exercises the registered scenario matrix.
