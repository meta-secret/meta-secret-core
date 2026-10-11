# Test #43 — distributed Secret creators and Android survivor

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault and Secret1; approves both Joins; passively observes its own removal |
| B | Android | Joins first; creates Secret2; approves Web removal; locally removes iOS; final survivor |
| C | iOS | Joins second; creates Secret3; requests Web removal; passively observes its own removal |

Run locally through the guarded visible runner with `npm run test-43:run`.

Web creates the Vault and `Secret1`, approves Android's Join, and Android
creates `Secret2` through visible UI. Web then approves iOS's Join and iOS
creates `Secret3`. The harness observes all three exact Secret rows and
discovers all three canonical registered Device names from unique Web rows
before allowing any removal.

At the three-to-two boundary, Android reaches a clean approval state, iOS
reaches a clean requester state, and Web reaches a passive-target boundary.
iOS performs a real left swipe on the exact Web row, verifies the exact
localized confirmation title, and persists the request. Android opens the
persisted incoming request, approves it, and performs reverse resharing. Web
must reset to registration with no stale Vault, Secret, Join, removal, or
recovery UI. Android and iOS each directly Show, verify, and close all three
Secrets.

At the two-to-one boundary, iOS reaches a passive-target state before Android
performs a real left swipe on the exact iOS row and completes the local
removal. iOS must reset to manual authentication. Android, the only remaining
Device, directly Shows all three Secrets again. This is the intended reading
of the original scenario's final sentence: the removed iOS Device cannot Show
after its own removal. The complete scenario has two removals, nine direct
Show cycles, and zero recovery Claims.

`iPhone 17` and `Pixel_4a` are runtime selectors only. Canonical Device names
are opaque values discovered from authoritative UI and propagated unchanged.
All waits are bounded by persisted UI state or Test43-specific markers. The
shared harness owns local selection, one-time builds, redacted diagnostics,
screenshots, cancellation, cleanup, and exact restoration. There are no
sleeps, `slowMo`, reduced cycles, or direct Core business actions.
