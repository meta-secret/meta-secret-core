# Test #31 — Android approves the second Join

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault, Secret1, Secret2, and (after cycle 18) Secret3 |
| B | Android | Joins first, verifies Secret1/Secret2 locally, and approves iOS joining |
| C | iOS | Sends the second Join request and verifies the redistributed Secrets |

Run the complete local visible scenario with `npm run test-31:run`.

The second Join deliberately belongs to Android B: iOS sends its request,
Android opens and approves it, and Web does not approve that request. Before
the Join, Android has observed both Web-created Secret1 and Secret2. The iOS
Join completes only after both rows are visible on iOS.

Secret1 and Secret2 are each recovered three times by Web, Android, and iOS
(18 cycles). Web then creates Secret3; Android and iOS must both expose its row
before cycle 19 starts. Secret3 is recovered with the same three-by-three
matrix (9 cycles). In every recovery both receivers expose the pending request,
the selected approver accepts it, the observer loses the request, and the
sender reveals the exact synthetic value and closes the dialog.

The resulting 27-cycle matrix gives each platform nine sender, nine approver,
and nine observer roles. There is no direct-Show phase, fixed sleep, `slowMo`,
or weakened assertion.
