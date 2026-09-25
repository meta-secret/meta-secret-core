# Test #44 — distributed Secret creators and iOS survivor

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates the Vault and Secret1; approves both Joins; passively observes its own removal |
| B | iOS | Joins first; creates Secret2; approves Android removal; locally removes Web; final survivor |
| C | Web | Joins second; creates Secret3; requests Android removal; passively observes its own removal |

Run locally through the guarded visible runner with `npm run test-44:run`.

Android creates the Vault and `Secret1`, approves iOS's Join, and iOS creates
`Secret2` through visible UI. Android then approves Web's Join and Web creates
`Secret3`. The harness observes all three exact Secret rows and discovers all
three canonical registered Device names from unique Web rows before allowing
any removal.

At the three-to-two boundary, Android reaches a clean passive-target state,
iOS reaches a clean approval state, and Web independently proves the same
membership before requesting Android removal. iOS opens the persisted incoming
request, verifies the exact localized confirmation title, approves it, and
performs reverse resharing. Android must reset to registration with no stale
Vault, Secret, Join, removal, or recovery UI. iOS and Web each directly Show,
verify, and close all three Secrets.

At the two-to-one boundary, Web reaches a passive-target state before iOS
performs a real left swipe on the exact Web row and completes the local
removal. Web must reset to registration. iOS, the only remaining Device,
directly Shows all three Secrets again. The complete scenario has two removals,
nine direct Show cycles, and zero recovery Claims.

`iPhone 17` and `Pixel_4a` are runtime selectors only. Canonical Device names
are opaque values discovered from authoritative UI and propagated unchanged.
All waits are bounded by persisted UI state or Test44-specific markers. The
shared harness owns local selection, one-time builds, redacted diagnostics,
screenshots, cancellation, cleanup, and exact restoration. There are no
sleeps, `slowMo`, reduced cycles, or direct Core business actions.
