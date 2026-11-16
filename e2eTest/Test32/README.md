# Test #32 — iOS approves the second Join

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates the Vault, Secret1, Secret2, and (after cycle 18) Secret3 |
| B | iOS | Joins first, verifies Secret1/Secret2 locally, and approves Web joining |
| C | Web | Sends the second Join request and verifies both existing Secrets |

Run the complete local visible scenario with `npm run test-32:run`.

The second Join deliberately belongs to iOS B: Web persists its request, iOS
opens and approves it, and Android does not approve that request. Before this
approval, the test proves iOS received Secret1 during its own Join and sees
Android-created Secret2. Web completes its Join only after both existing rows
are observable.

Secret1 and Secret2 are each recovered three times by Android, iOS, and Web
(18 cycles). Android then creates Secret3; iOS and Web must both expose its row
before cycle 19 starts. Secret3 is recovered with the same three-by-three
matrix (9 cycles). In every recovery both receivers expose the pending request,
the selected approver accepts it, the observer loses the request, and the
sender reveals the exact synthetic value and closes the dialog.

The resulting 27-cycle matrix gives every platform nine sender, nine approver,
and nine observer roles. There is no direct-Show phase, fixed sleep, `slowMo`,
stale-marker reuse, or weakened assertion.


## Scoped recovery orchestration

Exact `test32:android-owner-creator-lifecycle` uses the actual current source descriptor and
independent expected12 before raw metadata. Its iOS sender uses the distinct
`recoveryShowPhase` procedure at the original input-return/already-visible
boundary. Same ordinary600/currentD clips prep180/value45/close60; no business
or security rule changes. Launch through normal range owner, for example
`npm run test-range -- --from 32 --to 32 --run-id <fresh-ascii-owner> --preserve-evidence`. Original
actions/roles/repetitions/value/Copy/Hide/results stay mandatory. Host controls
are not whole UI or SDK/provenance acceptance.
