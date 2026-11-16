# Test #27 — iOS / Web / Android secondary-creator lifecycle

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and Secret1; approves both joins |
| B | Web | Joins first; creates Secret2 and Secret3 |
| C | Android | Joins after the 12 direct-Show cycles |

Run locally with `npm run test-27:run`.

iOS creates Secret1, then Web joins the Vault and creates Secret2. iOS and Web each show
both Secrets three times: 12 exact-value direct-Show cycles. Android then
joins. Secret1 and Secret2 each exercise iOS, Web, and Android as the sender
three times. Web creates Secret3 after cycle 30, and the same nine-cycle
recovery matrix is repeated for it.

Every recovery waits until both receivers expose the pending request. The
selected approver accepts it, the explicitly named observer removes its badge
and open action, and the sender reveals the exact expected synthetic value and
closes the dialog. The complete scenario is 12 direct-Show cycles plus 27
recovery cycles. Every platform acts nine times as sender, approver, and
observer. The shared orchestrator recycles only the Web renderer after
completed cycles 21 and 30 while preserving the same Web device state.


## Scoped recovery orchestration

Exact `test27:ios-secondary-creator-lifecycle` uses the actual current source descriptor and
independent expected12 before raw metadata. Its iOS sender uses the distinct
`recoveryShowPhase` procedure at the original input-return/already-visible
boundary. Same ordinary600/currentD clips prep180/value45/close60; no business
or security rule changes. Launch through normal range owner, for example
`npm run test-range -- --from 27 --to 27 --run-id <fresh-ascii-owner> --preserve-evidence`. Original
actions/roles/repetitions/value/Copy/Hide/results stay mandatory. Host controls
are not whole UI or SDK/provenance acceptance.
