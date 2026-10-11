# Test #47 — declined removals preserve Vault and recovery

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates Vault + Secret1; approves both Joins; declines Android's removal request; requests Android removal; performs recovery as sender/approver/observer |
| B | Android | Joins first; creates Secret2; declines iOS's removal request; requests iOS removal; performs recovery as sender/approver/observer |
| C | iOS | Joins second; creates Secret3; requests Web removal; declines Web's removal request; performs recovery as sender/approver/observer |

Run locally through the guarded visible runner with `npm run test-47:run`.

All three removal requests are terminally **Declined** through visible UI:

1. iOS requests removal of Web; Android declines.
2. Android requests removal of iOS; Web declines.
3. Web requests removal of Android; iOS declines.

After every decline, the pending request must disappear, all three canonical
Device rows must remain members, all three Secret rows must remain available,
and every platform recovers every Secret exactly once. This gives 9 recovery
cycles per phase and 27 total. Before approval both receivers expose the same
Claim; the observer clears after the selected approver finishes; the sender
reveals the exact synthetic value and closes the dialog.

The deterministic matrix is balanced per phase: Web sender uses
Android/iOS/Android, Android sender uses iOS/Web/iOS, and iOS sender uses
Web/Android/Web as approvers for Secret1/2/3. Across all phases each platform
is sender, approver, and observer exactly nine times.

`Pixel_4a` and `iPhone 17` are launch selectors only. Exact canonical Device
names are discovered from authoritative Web rows and propagated unchanged.
Every wait is bounded by visible UI, persisted state, or a completed-action
marker. There are no sleeps, `slowMo`, reduced cycles, remote services, or
direct Core business actions.
