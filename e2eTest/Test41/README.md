# Test #41 — Android owner, iOS Secret creator, Web removal approver

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates the Vault and Secret1, approves both Joins, requests iOS removal, then becomes the final removal target |
| B | iOS | Joins first, creates Secret2 and Secret3, then passively observes its own removal |
| C | Web | Joins second, approves iOS removal and reverse resharing, then locally removes Android |

Run locally through the guarded visible runner with `npm run test-41:run`.

Android creates the Vault and `Secret1`, approves iOS's Join, and iOS creates
`Secret2` through visible UI. Android then approves Web's Join, after which iOS
creates `Secret3`. The harness waits for all three exact Secret rows and exact
canonical Device rows before removal starts.

At the three-to-two boundary, iOS reaches a clean passive-target state,
Android reaches a clean requester state, and Web independently verifies the
same topology. Android performs a real left swipe and exact confirmation for
iOS. Web reloads only after Android's persisted request marker, opens the
incoming request, verifies the exact iOS target, and is the sole approver and
reverse resharer. iOS must reset to manual authentication with no stale Vault,
Secret, Join, removal, or recovery UI. Android and Web then directly Show,
verify, and close all three Secrets.

At the two-to-one boundary, Android is ready as the passive target before Web
uses the exact Android row, trash action, and confirmation dialog. Android must
reset to registration. Web then directly Shows all three Secrets again. The
complete scenario has two removals, nine direct Show cycles, and zero recovery
Claims.

`iPhone 17` and `Pixel_4a` are launch selectors only. Web discovers the exact
registered Device names and passes those opaque values unchanged to native
assertions. All waits are bounded by persisted UI state or Test41-specific
completion markers. The shared harness owns local selection, one-time builds,
fail-together cancellation, redacted diagnostics, screenshots, cleanup, and
exact restoration. There are no sleeps, `slowMo`, reduced cycles, or direct
Core business actions.
