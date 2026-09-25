# Test #38 — Android-owned membership removal and resharing

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates the Vault and all three Secrets; approves both Joins; initiates both removals |
| B | iOS | Joins first; exclusively approves removal of Web; later observes its own removal |
| C | Web | Joins second; observes its own removal and reset to authentication |

Run locally through the guarded visible runner with `npm run test-38:run`.

Android creates `Secret1`, approves iOS's Join, creates `Secret2`, approves
Web's Join, and creates `Secret3`. The harness discovers the exact canonical
Device names from Web's authoritative three-member UI; emulator/simulator
aliases are never treated as registered identities.

At the three-to-two boundary, iOS and Web reach clean observer/receiver
states before Android is released to swipe the exact Web row, expose the red
Remove action, and confirm the dialog. iOS opens the exact pending-removal row
and approves only after the request is visibly persisted. Web must reset to
manual authentication with no Vault tabs, Secret rows, removal UI, or recovery
UI. Android and iOS then each directly Show, verify, and close every Secret.

At the two-to-one boundary, iOS first reaches a clean target-ready state.
Android then swipes and confirms the exact iOS row through the local path;
there is no remote approval. iOS resets to authentication, and Android directly
Shows all three Secrets again. The complete scenario has two removals, nine
direct Show cycles, and zero recovery Claims.

All waits are bounded by persisted UI state or run-specific completion
markers. The shared orchestrator performs one local environment selection,
one build per target, visible-client readiness, fail-together cancellation,
redacted diagnostics, screenshots, cleanup, and exact environment restoration.
