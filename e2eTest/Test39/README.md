# Test #39 — iOS-owned membership removal and resharing

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and all three Secrets; approves both Joins; initiates both removals |
| B | Web | Joins first; exclusively approves removal of Android; later observes its own removal |
| C | Android | Joins second; passively observes its own removal and resets to authentication |

Run locally through the guarded visible runner with `npm run test-39:run`.

iOS creates `Secret1`, approves Web's Join, creates `Secret2`, approves
Android's Join, and creates `Secret3`. The harness discovers all three exact
canonical Device names from Web's authoritative three-member UI. Runtime
aliases such as `iPhone 17` and `Pixel_4a` are selectors only and are never
used as registered Device identities.

At the three-to-two boundary, Android reaches a clean passive-target state,
Web reaches a clean receiver state, and iOS reaches a clean requester state
before the removal is released. iOS swipes the exact Android member row,
opens the red Remove action, verifies the exact localized confirmation title,
and persists the request. Web then opens the exact Android row and confirms
the already-pending removal; it is the sole approver and reverse resharer.
Android must reset to manual authentication with no Vault, Secret, removal,
Join, or recovery UI. iOS and Web each directly Show, verify, and close all
three Secrets.

At the two-to-one boundary, Web first remains visible as the exact passive
target. iOS swipes its exact row and confirms the local removal, with no remote
approval action. Web resets to registration and iOS directly Shows all three
Secrets again. The complete scenario contains two removals, nine direct Show
cycles, and zero recovery Claims.

All actions use persisted UI state or run-specific bounded markers. The
shared harness performs local-only environment selection, one build per
target, exact restoration, fail-together cancellation, and redacted failure
diagnostics. There are no sleeps, `slowMo`, reduced repetitions, direct Core
business calls, or assumptions that a hidden swipe action proves visibility.
