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
