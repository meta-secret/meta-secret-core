# Test #30 — distributed Secret creators

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and Secret1; approves both joins |
| B | Web | Joins first and creates Secret2 |
| C | Android | Joins second and creates Secret3 |

Run the complete local visible scenario with `npm run test-30:run`.

The setup verifies every redistribution boundary through observable UI state:
Web receives Secret1 after joining, iOS sees Web-created Secret2 before Android
joins, Android receives Secret1 and Secret2, and both iOS and Web see
Android-created Secret3 before recovery begins.

Each platform then recovers each of the three Secrets three times, producing
27 recovery cycles. In every cycle both receivers must expose the pending
request before the selected approver acts. The observer must then lose the
request badge/open action, and the sender must reveal the exact expected
synthetic value and close the dialog.

The approval rotation is balanced: Web, Android, and iOS each act nine times
as sender, nine times as approver, and nine times as observer. The scenario has
no direct-Show phase and does not use fixed sleeps or `slowMo`; synchronization
uses persisted UI/native markers with bounded diagnostic deadlines.
