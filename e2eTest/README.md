# Meta Secret E2E tests

The tests in this directory orchestrate the local server, visible Web browser, and platform simulators.

Before changing environment selectors or building clients, the guarded runner
checks Docker and local ports, Android SDK Platform Tools and Emulator, the
scenario's named Android AVD, the exact iOS simulator, and the visible Xcode
device UI. Android SDK tools are discovered through `ANDROID_SDK_ROOT`,
`ANDROID_HOME`, or `~/Library/Android/sdk`; their directories are added to
child-process `PATH` automatically. At client readiness the runner starts the
AVD if needed, waits for boot and unlock, and boots the iOS simulator when
needed. `task set-env -- local` selects local endpoints and builds native
libraries; the E2E runner owns simulator discovery and startup.

## Commands

Install dependencies and run:

```bash
npm install
npm run test-1
npm run test-2
npm run test-3
npm run test-range -- --from 1 --to 49
npm run clean-space -- --force
```

`test-range` runs each registered test serially, chooses its declared
multi-block script when one exists, and pauses at the first failed test with
its evidence and queue position preserved. The AI coordinator classifies the
failure, repairs and reruns non-production failures, then resumes the range
with the same run ID. A confirmed production defect stops the queue for the
user's direction; a test is never skipped or counted as passed without a
successful run. Test #18 is
skipped only while it is undefined. Before each test it ensures the exact
scenario Android AVD and iOS simulator are booted and visible, writes a
readiness report, and monitors device availability until the scenario exits.
Healthy runtimes stay booted across tests and blocks; the runner resets
scenario app/server data between tests and verifies app/test-runner installation
before the first business action. At range completion, it shuts down only
simulators it started, after all platform actors have exited. Run manifests,
readiness JSON, and redacted per-test logs are created under
`.artifacts/runs/<run-id>/`. On low disk, the runner invokes
`clean-ui-e2e-space.mjs` with the allowlist; it pauses only when cleanup leaves
less than 12 GiB free. Resume with the same `--run-id` and the pending test
number. On a fully green batch, passing-attempt raw logs are pruned; failed
attempt logs and readiness reports remain available for analysis.

The cleaner preserves reusable Cargo/native/CLI outputs, Xcode build data and
Gradle caches between tests. It first removes other approved caches and measures
free space again. Only if less than the required 12 GiB remains may it remove
build caches, checking after Xcode and Gradle cleanup and using `cargo clean`
as the last fallback. The preferred 15 GiB trigger and `--force` do not override
this policy. Active builds or an unavailable process inventory block this
fallback; provenance checks rebuild and verify any removed output before use.

The public AI entry point remains `run_ui_e2e(test=all)`; it owns the cleaner,
local-environment, platform-supervisor, failure-analysis, and cleanup handoffs.
The npm range script supplies deterministic sequential process and artifact
mechanics and is not a GitHub Actions workflow.

The shared Test #4 harness used by Tests #4–17, #19, and #21–49 handles
signals and scenario failures through one shutdown barrier. It cancels owned
actors and marker waits, saves failure diagnostics once, and only then takes
the final process-tree snapshot and tears down the runtime. Late scenario
commands cannot start during shutdown. Owned descendants remain tracked even
if their native wrapper exits before them. The regression gate is
`node --test shared/harness-shutdown.test.mjs`.

Android preparation checks full Binder/package-manager readiness before
reset/install. Active-scenario monitoring instead checks exact AVD/online
transport, boot completion, owned emulator exit and a bounded shell heartbeat;
it does not repeat installation inventories while instrumentation is running.
Application correctness remains checked by scenario markers, actor results and
crash assertions. First-failure diagnostics can still collect bounded inventory
status/counts. This distinction is covered by `shared/platform-supervisor.test.mjs`.

iOS preparation keeps full installed-simulator discovery, Booted-state and
app/test installation checks. Active-scenario monitoring executes a bounded
15-second runtime-resolved `launchctl managerpid` on the exact selected UDID
and requires one integer guest manager PID greater than 1;
it does not repeatedly list all simulators during XCTest. Guest failure or
timeout still fails health under the existing retries, and XCTest/UI assertions
continue to check application behavior.
Preparation verifies this same guest command after bootstatus before handoff,
so an unsupported executable fails before business actions. Absolute host
executable paths are not assumed to exist inside the iOS runtime.

Long-run Web scenarios share `shared/web-page-recycle.mjs`: at an already
completed boundary they open a fresh page in the same context, clone the
target-scoped virtual passkey, unlock and verify the expected Secrets, then
close the old page and adopt the replacement. Test #22 now uses this same
mechanism as Test #23 at its existing cycles 12/21/30; all 39 cycles remain.
Browser diagnostic listeners are installed on each replacement, and failure
screenshots select the remaining current page from the browser context.
The shared default also includes a positive declared `createSecret3AfterCycle`
boundary. Tests #31–33 and #35–36 therefore recycle after completed cycle 18,
with the existing two Secrets verified, before creating the third Secret at
the beginning of cycle 19; Test #34 uses its declared cycle 12 boundary.
The existing cycles 21/30 remain in the policy;
initial creation at cycle 0 does not add a recycle boundary.
Declared `recovery.webPageRefreshAfterCycles` also marks settled membership or
recovery phase boundaries: Test #46 uses 6/15/24 after its direct Shows and
requester groups; Tests #47–49 use 9/18 after each completed recovery phase.
The shared helper merges these with 21/30 and any Secret creation boundary.
Every actor result, exact value check and dialog-close assertion must finish
before replacement; phase closures adopt the mutable replacement page.
`node --test shared/web-page-recycle.test.mjs` covers ownership, ordering and
failure behavior; a failed replacement retains the old page/authenticator.

## Test #1 — Web → iOS → Android

The orchestrator builds a fresh local server image, starts it on port `3000`, starts the Web client on port `5173`, and opens a visible Chromium window. It creates the Vault and Secret, verifies Web `Show`, then runs the iOS and Android join flows. Web approves both requests.

Test #1 is a recovery-lifecycle regression scenario. It runs consecutive recovery cycles with iOS approval, Android approval, and alternating approvers. Each cycle verifies that both receiver devices receive the request, the selected device approves it, Web reveals the recovered secret, and the other receiver closes its alert after recovery completes. A short pause separates cycles to expose stale-claim, status-synchronization, and request-ordering defects.

Before Web creates its Vault, Test #1 boots Android, installs the app and
instrumentation APKs, and verifies both the application package and registered
instrumentation. An Android readiness failure therefore stops before business
data is created.

The iOS simulator name is configured in `scenarios/test-1.json`. The test reuses that existing simulator: it does not clone devices and does not erase simulator content/settings. Before each iOS launch it boots the simulator if needed and uninstalls `org.metasecret.vault`.

Android is configured there too (`Pixel_4a` by default). The range supervisor reuses the existing AVD and starts it only when it is not already running. The allowlisted disk cleaner may reset this approved AVD with `-wipe-data` when space is low and it is not online; the E2E runner then reinstalls its app and instrumentation. The ordinary scenario orchestrator never wipes an AVD; before each Android run it uninstalls `metasecret.project.com`.

When run standalone, Test #1 leaves the browser open after success so the flow
can be observed. Under `test-range`, the range runner owns the shared lifecycle
and closes the scenario browser only after all platform actors have completed.

## Test #2 — iOS → Android → Web

iOS creates the vault and secret, approves Android and Web join requests, then Web runs the same recovery lifecycle as Test #1.

## Test #3 — Android → Web → iOS

Android creates the vault and secret, approves Web and iOS join requests, then Web runs the same recovery lifecycle as Test #1.

## Test #4 — Concurrent recovery across Web, iOS, and Android

Test #4 runs three declared blocks sequentially. Each block starts a fresh local
server and fresh Web context, removes the participating iOS and Android app
packages before reinstalling them, and verifies the same simulator and AVD
remain booted and ready. The reset clears app-local data; it does not erase
simulator-wide settings or Keychain contents.

Android reports a Web join as approved only after the exact candidate appears
in its Core-backed device state with `Member` status. For Web recovery
approvals, the local runner enables `META_SERVER_E2E_SIGNALS=1`. The server
publishes an authenticated SSE signal containing the exact `claimId` and
decision after persisting it; the orchestrator matches that signal to the
opened approval dialog and then checks the Web UI state. The signal is disabled
by default and carries no Secret or Key Share data. Event signals drive the
scenario; bounded deadlines only diagnose a signal that never arrives.

## iOS Core provenance gate (all UI E2E tests)

Before any simulator startup, require local Core configuration and run the shared
`e2eTest/shared/ios-ffi-provenance.mjs` gate. It calls the official
`meta-secret/mobile/scripts/build-ios.sh` when source/archive/generated Swift
binding provenance is absent or stale. Do not trust a successful Xcode build
as evidence that its Rust archive is current. Reuse within a batch requires
matching current source, both archive hashes and both generated binding trees.
Record build output and provenance in the run artifacts.

Before `build-for-testing`, verify expanded simulator linker settings select
`iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a`. Invalidate DerivedData
when native/project inputs differ from the recorded build. Record app and XCTest
product hashes separately from static archive hashes, and verify these before
installation or `test-without-building`. The range and direct Test1–3 runners
and the shared Test4 runner used by later scenarios all use this gate.

Environment snapshots must preserve the newly verified iOS artifacts across
scenario blocks; restore the user's original artifacts only when the batch
finishes. A stale or unverifiable bundle is a preparation failure and cannot
start business actions. This changes orchestration only: Constraints and
Glossary domain semantics are unchanged, including exact membership-request
comparison and claim generation binding.
