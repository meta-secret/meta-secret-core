# Test #45 — distributed Secret creators and Web survivor

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and Secret1; approves both Joins; passively observes its own removal |
| B | Web | Joins first; creates Secret2; approves iOS removal; locally removes Android; final survivor |
| C | Android | Joins second; creates Secret3; requests iOS removal; passively observes its own removal |

Run locally through the guarded visible runner with `npm run test-45:run`.

iOS creates the Vault and `Secret1`, approves Web's Join, and Web creates
`Secret2` through visible UI. iOS then approves Android's Join and Android
creates `Secret3` through visible UI. The harness observes all three exact
Secret rows and discovers all three canonical registered Device names from
unique Web rows before allowing any removal.

At the three-to-two boundary, iOS reaches a clean passive-target state,
Android reaches a clean requester state, and Web independently proves the
same membership before approving iOS removal. Web opens the persisted request,
verifies the exact target, approves it, and performs reverse resharing. iOS
must reset to registration with no stale Vault, Secret, Join, removal, or
recovery UI. Android and Web each directly Show, verify, and close all three
Secrets.

At the two-to-one boundary, Android reaches a passive-target state before Web
uses the exact Android row and confirmation dialog to complete the local
removal. Android must reset to registration. Web, the only remaining Device,
directly Shows all three Secrets again. The complete scenario has two removals,
nine direct Show cycles, and zero recovery Claims.

`iPhone 17` and `Pixel_4a` are runtime selectors only. Canonical Device names
are opaque values discovered from authoritative UI and propagated unchanged.
All waits are bounded by persisted UI state or Test45-specific markers. The
shared harness owns local selection, one-time builds, redacted diagnostics,
screenshots, cancellation, cleanup, and exact restoration. There are no
sleeps, `slowMo`, reduced cycles, or direct Core business actions.
