# Test #26 — Android / iOS / Web secondary-creator lifecycle

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates the Vault and Secret1; approves both joins |
| B | iOS | Joins first; creates Secret2 and Secret3 |
| C | Web | Joins after the 12 direct-Show cycles |

Run locally with `npm run test-26:run`.

Android creates Secret1 and joins iOS. iOS creates Secret2. Android and iOS
each show both Secrets three times: 12 exact-value direct-Show cycles. Web then
joins. Secret1 and Secret2 each exercise Android, iOS, and Web as the sender
three times. iOS creates Secret3 after cycle 30, and the same nine-cycle
recovery matrix is repeated for it.

Every recovery waits until both receivers expose the pending request. The
selected approver accepts it, the explicitly named observer removes its badge
and open action, and the sender reveals the exact expected synthetic value.
The complete scenario is 12 direct-Show cycles plus 27 recovery cycles. The
shared orchestrator recycles only the Web renderer after completed cycles 21
and 30 while preserving the same Web device storage and passkey.
