# Test #25 — Web / Android / iOS secondary-creator lifecycle

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault and Secret1; joins both devices |
| B | Android | Joins first; creates Secret2 and Secret3 |
| C | iOS | Joins after the 12 direct-Show cycles |

Run locally with `npm run test-25:run`.

Web creates Secret1 and joins Android. Android creates Secret2. Web and
Android each show both Secrets three times: 12 exact-value direct-Show cycles.
iOS then joins. Secret1 and Secret2 each exercise Web, Android, and iOS as the
sender three times. Android creates Secret3, and the same nine-cycle recovery
matrix is repeated for it. Every recovery waits until both receivers expose
the pending request, the selected approver accepts it, the observer removes
its badge/open action, and the sender reveals the exact expected value.

The complete scenario is 12 direct-Show cycles plus 27 recovery cycles. The
shared orchestrator recycles the Web renderer after completed cycles 21 and 30
while preserving the same browser context, device storage, and passkey.
