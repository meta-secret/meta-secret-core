# Test #42 — iOS owner, Web Secret creator, Android removal approver

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and Secret1, approves both Joins, and requests Web removal |
| B | Web | Joins first, creates Secret2 and Secret3, then passively observes its own removal |
| C | Android | Joins second, approves Web removal and reverse resharing, then locally removes iOS |

Run locally through the guarded visible runner with `npm run test-42:run`.

iOS creates the Vault and `Secret1`, then approves Web's Join. Web creates
`Secret2` through visible UI before Android joins. iOS is the deterministic
sole approver of Android's Join, after which Web creates `Secret3`. The harness
waits for all three exact Secret rows and all three exact canonical Device
rows before removal begins.

At the three-to-two boundary, Web reaches a clean passive-target state,
Android reaches a clean approval state, and iOS reaches a clean requester
state. iOS performs a real left swipe on the exact Web row, verifies the exact
localized confirmation title, and persists the request. Android opens the
persisted incoming request and is the sole approver and reverse resharer. Web
must reset to registration with no stale Vault, Secret, Join, removal, or
recovery UI. iOS and Android then each directly Show, verify, and close all
three Secrets.

At the two-to-one boundary, iOS is ready as the passive target before Android
performs a real left swipe on the exact iOS row and completes the local
removal. iOS must reset to manual authentication. Android then directly Shows
all three Secrets again. The complete scenario has two removals, nine direct
Show cycles, and zero recovery Claims.

`iPhone 17` and `Pixel_4a` are runtime selectors only. Web discovers the exact
registered Device names from one unique platform row each and propagates those
opaque values unchanged to native assertions. All waits are bounded by
persisted UI state or Test42-specific markers. The shared harness owns local
selection, one-time builds, fail-together cancellation, redacted diagnostics,
screenshots, cleanup, and exact restoration. There are no sleeps, `slowMo`,
reduced cycles, or direct Core business actions.
