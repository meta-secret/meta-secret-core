# Test #37 — Web-owned membership removal and resharing

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault and all three Secrets; approves both Joins; initiates both removals |
| B | Android | Joins first; exclusively approves removal of iOS; later observes its own removal |
| C | iOS | Joins second; observes its own removal and reset to authentication |

Run locally through the guarded visible runner with `npm run test-37:run`.

Web creates `Secret1`, approves Android's Join, and creates `Secret2`. iOS then
requests the second Join and Web is the deterministic sole approver; Android
receives no Join-approval action. Web creates `Secret3` only after the
three-device membership is visible on all clients.

The first removal is a three-to-two transition. Android and iOS reach clean,
three-member UI-ready barriers before Web uses the visible iOS device row,
trash action, and confirmation dialog. Android must expose the exact pending
iOS-removal row and localized `Remove device <iPhone name>?` title before the
orchestrator releases its visible approval. Web reads the exact nested device
name from the selected row and requires the confirmation's nested target-name
element to match it exactly. iOS makes no decision and must transition to
authentication with no Vault tabs, Secret rows, or stale removal/recovery UI.

`simulatorName` and `avdName` select the local iOS/Android runtimes only. They
are never treated as Vault member identities. At the three-member boundary the
harness discovers each canonical registered name from one unique exact Web
platform row, requires one non-empty nested name, and propagates those observed
names unchanged to the Web confirmation and both native assertions. Thus an
iPhone 17 simulator may correctly register as `iPhone18,3` without weakening
the identity check or using substring matching.

After resharing, Web and Android each Show `Secret1`, `Secret2`, and `Secret3`
once, assert the exact synthetic value, and close the dialog. These six
checks are direct Show actions and create no recovery Claim.

The second removal is the two-to-one transition. Android first reaches a
clean target-ready barrier. Web then removes Android with its local visible
confirmation/passkey path; no remote approval process or marker is created.
Android must reset to authentication. Web finally Shows and closes all three
exact Secrets. Every membership boundary verifies visible participant rows,
member counts, and absence of stale Join, removal, and recovery UI. Web scopes
`Current` plus no trash to its exact row and `Member` plus one own trash action
to each Android/iOS row. Native readiness uses the same Web-observed exact
device names: Android pairs them with existing `device-row-<id>` and
`remove-device-<id>` semantics, while iOS pairs exact AX label components with
the corresponding row/action identifiers. The 3→2 and 2→1 boundaries assert
the exact remaining role/action topology.

The shared harness supplies the deterministic local-only environment guard,
whole native-artifact snapshot/restore, state-driven process cleanup,
stateful diagnostic redaction, sanitized screenshots, lossless POSIX quoting
for every direct Android instrumentation argument, and one iOS
`build-for-testing` reused by all iOS steps. The runner invokes adb as
`shell -- <one remote command>`; diagnostic argv is independently redacted at
the logical key/value layer before serialization, so quoted command fragments
cannot reconstruct Secret, private/master key, or Key Share values. Android
markers come from logcat, but the instrumentation process remains the
authoritative public result, so the expected logcat SIGTERM cannot mask its
exit/error. There are no sleeps, `slowMo`, or direct Core business actions.
