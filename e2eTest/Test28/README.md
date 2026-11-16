# Test #28 — distributed Secret creators

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault and Secret1; joins Android and iOS |
| B | Android | Joins first and creates Secret2 |
| C | iOS | Joins second and creates Secret3 |

Run the complete local visible scenario with `npm run test-28:run`.

After all three Secrets have been persisted and synchronized, each platform
recovers each Secret three times. This produces 27 recovery cycles. In every
cycle both receivers must expose the pending request before the selected
approver acts. The observer must then lose the request badge/open action, and
the sender must reveal the exact expected synthetic value and close the
dialog.

The approval rotation is balanced: Web, Android, and iOS each act nine times
as sender, nine times as approver, and nine times as observer. The scenario has
no direct-Show phase and does not use fixed sleeps or `slowMo`; synchronization
uses persisted UI/native markers with bounded diagnostic deadlines.


## Scoped recovery orchestration

Exact `test28:web-secondary-creator-lifecycle` uses the actual current source descriptor and
independent expected12 before raw metadata. Its iOS sender uses the distinct
`recoveryShowPhase` procedure at the original input-return/already-visible
boundary. Same ordinary600/currentD clips prep180/value45/close60; no business
or security rule changes. Launch through normal range owner, for example
`npm run test-range -- --from 28 --to 28 --run-id <fresh-ascii-owner> --preserve-evidence`. Original
actions/roles/repetitions/value/Copy/Hide/results stay mandatory. Host controls
are not whole UI or SDK/provenance acceptance.
