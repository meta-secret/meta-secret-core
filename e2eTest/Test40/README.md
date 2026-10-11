# Test #40 — Web owner, Android Secret creator, iOS removal approver

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault and Secret1, approves both Joins, requests Android removal, then becomes the final removal target |
| B | Android | Joins first, creates Secret2 and Secret3, then passively observes its own removal |
| C | iOS | Joins second, approves Android removal and reverse resharing, then locally removes Web |

Run locally through the guarded visible runner with `npm run test-40:run`.

Web creates the Vault and `Secret1`, then approves Android's Join. Android
creates `Secret2` through visible UI before iOS joins. Web is the deterministic
sole approver of the iOS Join, after which Android creates `Secret3`. The
harness waits until all three exact Secret rows and all three exact canonical
Device rows are observable before any removal begins.

At the three-to-two boundary, Android reaches a clean passive-target state and
iOS reaches a clean approval state before Web opens the exact Android row,
trash action, and confirmation dialog. iOS opens the persisted incoming
removal request and is the sole approver and reverse resharer. Android must
reset to manual authentication with no Vault, Secret, Join, removal, or
recovery UI. Web and iOS then each directly Show, verify, and close all three
Secrets.

At the two-to-one boundary, Web is ready as the passive target before iOS
performs a real left swipe on the exact Web row, exposes Remove, verifies the
exact localized confirmation title, and completes the local removal. Web must
reset to registration. iOS then directly Shows all three Secrets again. The
complete scenario has two removals, nine direct Show cycles, and zero recovery
Claims.

`iPhone 17` and `Pixel_4a` are launch selectors only. Web discovers the exact
registered Device names from one unique platform row each and propagates those
opaque values unchanged to native assertions. All waits are bounded by
persisted UI state or Test40-specific completion markers. The shared harness
performs one local environment selection when required, one build per target,
fail-together cancellation, redacted diagnostics, screenshots, cleanup, and
exact environment restoration. There are no sleeps, `slowMo`, reduced cycles,
or direct Core business actions.
