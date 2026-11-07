# Test #48 — rotated declined removals preserve Vault and recovery

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates Vault + Secret1; approves both Joins; is the first removal target; declines iOS's removal request; requests iOS removal |
| B | iOS | Joins first; creates Secret2; declines Web's removal request; requests Web removal; is the final removal target |
| C | Web | Joins second; creates Secret3; requests Android removal; is the second removal target; declines Android's removal request |

Run locally through the guarded visible runner with `npm run test-48:run`.

All three removal requests are terminally **Declined** through visible UI:

1. Web requests removal of Android; iOS declines.
2. iOS requests removal of Web; Android declines.
3. Android requests removal of iOS; Web declines.

After every decline, the pending request disappears, all three exact Device
rows remain members, and no stale recovery/removal UI remains. Each platform
then recovers Secret1, Secret2, and Secret3 once: 9 cycles per phase and 27 in
total. Both receivers expose the same request before approval, the observer
clears after the selected approver responds, and the sender verifies the exact
synthetic value before closing the dialog.

The deterministic recovery matrix is balanced per phase: Web sender uses
Android/iOS/Android, Android sender uses iOS/Web/iOS, and iOS sender uses
Web/Android/Web as approvers for Secret1/2/3. Across all phases each platform
is sender, approver, and observer exactly nine times.

`Pixel_4a` and `iPhone 17` are launch selectors only. Canonical Device names
come from authoritative Web rows. All waits are bounded by visible UI,
persisted state, or completion markers; there are no sleeps, `slowMo`, reduced
cycles, remote services, or direct Core business actions.


## Scoped recovery orchestration

Exact `test48:android-distributed-creator-declined-removal-recovery-lifecycle` uses the actual current source descriptor and
independent expected12 before raw metadata. Its iOS sender uses the distinct
`recoveryShowPhase` procedure at the original input-return/already-visible
boundary. Same ordinary600/currentD clips prep180/value45/close60; no business
or security rule changes. Launch through normal range owner, for example
`npm run test-range -- --from 48 --to 48 --run-id <fresh-ascii-owner> --preserve-evidence`. Original
actions/roles/repetitions/value/Copy/Hide/results stay mandatory. Host controls
are not whole UI or SDK/provenance acceptance.
