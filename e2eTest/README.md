# Meta Secret E2E tests

The tests in this directory orchestrate the local server, visible Web browser, and platform simulators.

Before changing environment selectors or building clients, the guarded runner
checks Docker and local ports, Android SDK Platform Tools and Emulator, the
scenario's named Android AVD, the exact iOS simulator, and the visible Xcode
device UI. Android SDK tools are discovered through `ANDROID_SDK_ROOT`,
`ANDROID_HOME`, or `~/Library/Android/sdk`; their directories are added to
child-process `PATH` automatically. At client readiness the runner starts the
AVD if needed, waits for boot and unlock, and boots the iOS simulator when
needed. One parent transaction snapshots all four selectors and original client
products, applies `task set-env -- local --selectors-only` once when needed,
and restores the original configuration/products in its final cleanup. The
official owning native/product builders run after that preparation; simulator
discovery and startup remain the runner's responsibility.

## Batch product proof

Before any clients start, builds are staged at most once per distinct current
source + configuration + target tuple. Android uses one common verified FFI
build covering `arm64-v8a`, `armeabi-v7a`, and `x86_64`, then one app/test APK
assembly for each required existing configuration:

| Configuration | Local Android socket and E2E server override |
|---|---|
| `defaultLocal` | Both blank; existing local selection |
| `network16` | Both `http://10.0.2.2:5181` |
| `deletion50` | Both `http://10.0.2.2:5201` |

Each app/test pair is immediately stashed with its exact tuple, all three ABI
checks and successful owning-builder receipt. Actors install that selected
pair; they do not use whichever working output another route last overwrote.
iOS retains the existing source/archive/generated-binding and expanded-link
proof, plus full app/XCTest source and product-tree proof. It builds the shared
bundle once with `build-for-testing`, then keeps the original selectors and
launch routing with `test-without-building`. Proven WASM/server products are
also reused; no build runs inside a business cycle.

A persisted same-run index is revalidated against current full inputs, products
and owning receipts before any rebuild path. Missing or changed proof for an
already prepared tuple fails closed and requires a deliberate full restart;
resume does not silently rebuild or restore native outputs from another run.
Standalone runners keep their owning build fallback.


Prospective range preparations with the actual active environment guard write
an explicit V4 `owning-f2-support-v1` index. Before its first seal, the owning
preparation preserves supporting bytes in that same run: JNI/generated
bindings and native proof, copied iOS archives/bindings, the original Xcode
link record and generated `.xctestrun`, consumed app/test products, CLI bytes
and mode, and working WASM. It does not copy DerivedData intermediates or host
Cargo archives/caches. The existing disk-copy budget applies before capture
and replay; missing or partial support is a hard failure, never a rebuild.

Only an exact current-process active guard covering the unchanged22 targets
can materialize V4 bytes back to their fixed original paths. Source, roots,
configuration, target, toolchain, original receipts, supporting hashes and
read-only Cargo archives/client-owned Swift compatibility files must still
match. All original native/link/template/ABI/CLI/image and pre-spawn validators
remain required afterwards. Consumers only verify; they never hydrate. Replay
records a separate verified-materialization receipt with zero builders, and
the same finally restores the user's original files and selectors, including
an original CLI different from the owning F2 CLI. Closing the guard revokes the
capability synchronously. Persisted/JSON/cloned objects cannot recreate it.

Existing V3 indexes retain their original strict checks and are never upgraded,
resealed or supplied invented supporting proof. V4 requires a genuine owning
F2 generation; it cannot rescue an old V3 after required working proof is lost.
The exceptional physical preparation restart still requires explicit admission;
a new schema or run ID does not grant it or reset application retry budgets.

Runtime cleanup similarly retains the exact current-process callback object.
Only that object's joined owned-child close can acknowledge an already closed
launch without another stop. Scalar manifest summaries are evidence, never
shutdown authority; a newer owner, borrowed runtime or unjoined child cannot
inherit the acknowledgement. Historical ownership records are not rewritten.

New guarded range preparations select V5. It preserves V4's complete supporting
bundle while separating compiled inputs from fresh runner validation. Known
noncompiled helper/test/document content may change without recompiling proven
products; build recipes, scenario JSON, file membership, unknown inputs, native
test hooks, application inputs and toolchain remain strict. Before preparation,
each current runner executes the fixed F1 syntax and related Node checks and
stores private hashed command/output receipts. Children verify that exact current
audit again at selection and before spawn. An old audit or V3/V4 index is never
upgraded or resealed. Binary reuse does not authorize reusing scenario results
after changing actions/assertions.

The E2E-only `e2e-16g-v1` profile uses one Gradle worker, no parallel Gradle work,
a 3 GiB JVM heap/768 MiB metaspace and Kotlin in-process, one Xcode job and two
Cargo jobs. Command-scoped flags also cover Gradle embedded in Xcode. Ordinary
developer settings are unchanged; single-use Gradle exits after the build and
warm Gradle/Cargo/Xcode caches remain available. Owning receipts record the
profile. A full preparation budget includes snapshot, all selected product
copies, working/cache allowance, supporting bytes, swap allowance and a 12 GiB
free reserve. Replay reserves only remaining copy/hash work; missing proof never
falls back to compilation. Free/inactive RAM pages are a qualified pressure
estimate, not a guarantee of future available memory. Unsafe budgets stop before
expensive work. Fixed numeric phase metrics record monotonic duration/free disk/
swap without arguments, environment values or secret content. Successful native
install records also contain `installDurationMs`.

Artifact-tree verification reads regular files through one reusable 1 MiB
buffer per hash call. The digest framing/order is unchanged; a premature EOF or
observed file identity/size/metadata change fails closed. This bounds the helper's
file buffer, not total runner RSS. Its fixed F1 regression checks cover legacy
digests, partial reads, drift and descriptor cleanup. Changing this strict recipe
still requires a fresh genuine product generation; an old sealed V5 bundle is
never rewritten to accommodate it.

Healthy existing guests stay up between tests. After actors drain at a pending
test pause, exact owned guests may remain running through an atomic health and
identity checked release. Its receipt records revocation, not transferable
shutdown authority. A new process borrows the guest. Failed health/uncertain
ownership retains existing cleanup; interruptions and completed ranges retain
their previous policy. No guest/profile is cloned, erased or recreated.

Test #20 has a typed participant-only profile. The parent checks cargo/sqlite,
the Core manifest/migration and absence of fixed `127.0.0.1:3000`, then seeds
pending CLI/Core and HTTP readiness. Its original child owns the temporary
SQLite database, migration, actual cargo meta-server, both Core/HTTP tests and
cleanup. It does not acquire invented Docker/Web/native configuration. A zero
child exit without both named prerequisite and exact healthy `/hi` proofs fails
readiness.

## Commands

Install dependencies and run:

```bash
npm install
npm run test-1
npm run test-2
npm run test-3
npm run test-range -- --from 1 --to 50
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

For bounded storage maintenance, optionally add `--pause-after-each-case` and
`--preserve-evidence`. After a whole test passes and its actor closes, the runner
journals that unchanged result, records the next canonical pending test with
status `paused-maintenance`, restores the environment and exits75. Failed,
readiness-failed and interrupted tests never advance. The last selected test
completes normally. Example initial command:
`npm run test-range -- --from 26 --to 49 --run-id OWNER --preserve-evidence --pause-after-each-case`.
Resume only from `run.json.pendingTest`, with the same OWNER and original endpoint:
`npm run test-range -- --from PENDING --to 49 --run-id OWNER --preserve-evidence --pause-after-each-case`.
Fresh lineage/environment/resource checks and sealed V5 same-owner product
verification still apply; verified replay performs no compiler rebuild. Never
edit sealed sources/proofs or a pending pointer to force continuation. A paused
owner remains resumable, not completed: no successful-run pruning or blanket
cleanup. Independently reviewed reversible archival is limited to immutable
fully passed/analyzed earlier-case diagnostics; protect raw logs, indexes,
audits, pending/failed cases and every restoration/product dependency, and
restore raw diagnostics before any raw consumer. A maintenance checkpoint does
not guarantee free space or cap the next test's diagnostic growth. Diagnostic
block mode cannot use this option. Existing cycle counts and deadlines remain.

The default range is 1–50: 49 defined IDs, with undefined #18 omitted. #20
retains its existing local Core/HTTP checks. Registration #51 remains outside
the default range and can still be selected explicitly. Existing inner cycle
counts are unchanged, including 39 each in #22–27 and #50's eight blocks,
60 cycles and three repetitions within each variant.

Liveboard Web adapters use the actual Create modal, inline Show/Hide cards,
disabled waiting actions and typed request identities in 04. The authored
source is not a test result. Visible incoming requests follow the actual FIFO head, without a request picker;
helpers never decide a foreign request. Test15 block1 keeps its original roles:
Android explicitly declines the visible Recover before approving Remove C; its
initial decline is an explicit FIFO response. Authored/F1 evidence is separate
from native/runtime acceptance. Browser
WebAuthn API abort is labelled separately from native OS Cancel-button evidence.
See the workspace [source handoff](../../.ai/artifacts/run/MS-20261019210543-e2e-web-phase1-source.md)
for the review and F1/runtime status.

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

The range reuses only the existing iPhone 17 / iOS 26.5 device
DB22263E-A8A3-49C5-ADD1-BAE4089E2E30 and registered Pixel_4a AVD, sequentially.
It never creates/clones/deletes/erases a simulator, creates a private AVD, resets
Keychain or wipes device userdata. Legacy owned-profile opt-ins, readiness,
creation intents and resume manifests are refused before any SDK/app/server
mutation or profile-cleaning finally. Historical creator/cleanup modules and
their controlled evidence do not authorize a new run.

Parent readiness binds exact UUID/runtime/type/name and the existing Android
registration/config/data path. Healthy guests stay booted across methods and
blocks; only a runtime started by this run may be stopped after actors drain.
Already running guests, user settings, credentials, Keychain and global DB
remain preserved. Android selected-package uninstall/reinstall and the named local
test-server reset retain their existing boundaries. iOS instead terminates only
the exact app and upgrade-installs; it never deletes the app container or legacy
Keychain/DB/preferences. Normal app UI Reset requires a proven exact test identity.

The public iOS range and Test1–4 callers use an isolated E2E build configuration:
METASECRET_E2E_IDENTITY_REQUIRED=YES in the configuration tuple, effective build
settings and owning build. Before install/reuse, the actual signed app Info.plist
must contain MetaSecretE2EIdentityRequired=YES. The ordinary project default stays
NO. A required build without a valid namespace fails before storage access.
A marked installed test binary cannot be launched normally from its icon without
that configuration; restoring an ordinary NO developer binary after the final
batch remains an explicit pending build/install step, without uninstall or wipe.

At each existing scenario/variant/attempt clean boundary, the parent creates one
canonical METASECRET_E2E_IDENTITY_NAMESPACE (f2-lowercase-UUID). A private 0600
record under the run directory binds the raw parent run ID, scenario path or
Test50 block ID, attempt and exact device/bundle. All actors and restarts reuse
that record; a missing/malformed/foreign actor record stops without replacement
or legacy fallback. Test50 keeps one scope per existing block, not per actor or
cycle: its original eight blocks and 60 cycles are unchanged. A fresh standalone
invocation has a fresh invocation ID. Failure retains the private scope and
transport copies for diagnosis; success-only UI Reset concerns its proven scope.

The immutable app scope selects the E2E Keychain service and named Settings suite;
the unchanged fresh master-key hash selects its DB filename. Each actual XCTest
uses a private 0600 copy of the owning generated xctestrun, with the same namespace
in host and app dictionaries, original-file hash and semantic inverse checks.
Namespace is redacted before streaming/persistence and is not supplied in argv.
Five existing XCTest class setup hooks propagate it; all original business methods
and scenario JSON remain unchanged. Test51/registration-only stays namespace-
ineligible and stops before preparation/reset/launch.

These caller/build/transport paths have controlled port coverage; public iOS
range1–50 has not run with this integration. Dedicated Saved/real-OS runtime
admission remains separate. Controlled branches retain their existing auth mode;
Test50 real-OS dispatch requires CaseEight plus persisted requireCoreRouting=true
and refuses METASECRET_UI_TEST_MODE or SIMCTL_CHILD_METASECRET_UI_TEST_MODE keys, including empty/false,
through ambient, private configuration and every generated-template dictionary.
Namespace never supplies enrollment/credentials or bypasses real Cancel/approval.

Test50 also accepts the explicit owner-approved `E2E_TEST50_AUTH_PROFILE=controlled-business`
selection. Its scoped invocation uses `authPolicy=controlled-business` only for
Test50/CaseEight with the same true Core routing and canonical existing simulator
identity; ordinary `controlled` still cannot carry true routing. Host/template
MODE aliases remain forbidden; XCTest adds MODE to its app only after admission.
The Python transport's strict-template Boolean is not an OS-result assertion.
Production authenticators, physical phones and original scenario JSON are unchanged.
Native cancellation returns manual pending before Delete. A receipt-backed8/60
business result retains OS confirmation/Cancel manual UNRUN/fullAcceptance=false,
including range/resume/output; it cannot become original Test50 or Saved/realOS
acceptance. See [Test50 profile contract](Test50/README.md).
Android app-reset and authentication behavior are unchanged.

The primary-only iOS gesture experiment keeps exact selected opaque-ID admission:
when the current nonempty `lb-primary-*` is not hittable, one element-relative
center coordinate tap replaces the native tap before any attempt. Unique enabled
button and current app/sheet/list/window geometry must admit it; refusal has no
fallback. Other controls and hittable primary buttons retain native taps. These
sequential rectangle checks do not prove unobscuredness or the eventual hit
receiver; a dispatched gesture is not business PASS or native AX/a11y acceptance.
Existing strict auth/value/claims/close postconditions and manual OS pending
classification remain authoritative; the six-role diagnostic is unchanged.

Every actual XCTest invocation explicitly disables parallel testing, limits
workers and concurrent simulator destinations to one, and selects that same UUID.
No new-instance retry/fallback is allowed. Source admission and controlled port
tests are not actual GUI/OS acceptance; fresh GUI/F1–F3 and genuine OS capability
remain required before the reviewed runtime lane.

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
Every actor result, exact value check and Hide/conceal assertion must finish
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

The iOS simulator name is configured in `scenarios/test-1.json`. The test reuses that existing simulator: it does not clone devices and does not erase simulator content/settings. Before each iOS launch it boots the simulator if needed, terminates only `org.metasecret.vault`, then upgrade-installs the required isolated build with its stable scoped identity.

Android uses the existing registered `Pixel_4a` AVD and starts it only when it is not already running. New/private AVDs and data wipes are forbidden, including low-disk cleanup and retries. The cleaner removes only approved caches; before each Android run the runner uninstalls the selected `metasecret.project.com` app and instrumentation packages while preserving device settings and credentials.

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
remain booted and ready. Default standalone targets clear app-local data without
erasing simulator-wide settings or Keychain contents. Keychain reset and device-wide erase are forbidden at every block boundary;
legacy owned-profile claims are refused before reset, not silently adopted.

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

Full-recovery iOS actors in Test1, Test2, Test3 and the generic Test4 tail observe suite
pass/failure and process exit from launch. Their existing600000ms (or original
Test4-selected360000ms) suite-completion allowance starts only at the final
completion handoff, after the declared business actions and close markers; an
early result subscription cannot start it. There is no separate whole-business
wall-clock watchdog: finite declared cycles and existing per-action bounds
remain. Other mode-specific/per-step Test4 actors retain their eager default bounds.
A real actor failure cancels owned Web/marker work, prevents the next approval,
cycle or spawn, and joins the losing business continuation. Native wrappers
retain the existing5s/5s/2s lifecycle. Browser close and losing-body join each
have a5000ms teardown bound; an unresolved port is a named retained cleanup
blocker, never a joined/drained or business PASS claim. The offline iOS terminal
boundary arms the same completion allowance before waiting for that invocation.
A required suite marker and strict process result/close remain mandatory;
primary and cleanup causes are retained through redacted recursive formatting.

Test16 network-loss iOS actors also use explicit completion: after all three
original cycles in a role block and their original value/Close/observer
postconditions, the owner starts the unchanged360000ms suite-finalization
allowance before awaiting any actor result. Every original per-action bound
remains. Selected-suite/process failure is observed from launch; the owner
latch is checked around awaited work and before deferred Web Approve callbacks
or later dispatch. An already-dispatched opaque UI operation is not claimed
physically cancelled. Every block failure cancels and joins its owned iOS actor,
retains the primary cause, and leaves other owned cleanup to the existing
shutdown barrier. A host check never replaces the required whole Test16 rerun.

Test22 iOS Show uses a private invocation-specific preparation event after the existing
Show tap and system-confirmation calls return, or at the existing already-visible
reveal return. Preparation is procedural evidence only. The exact plaintext or
ordered seed and Copy assertions must still finish before the original visible
marker; the original Close marker and both real actor results remain required.
The old45s wait began at approval and included mandatory full-list selection. This
harness phase correction allows up to180s preparation, then the original45s exact
value-marker wait, then the original60s Close wait; each is capped by the remaining
original ordinary600000ms selected-suite deadline. That deadline is sampled at
its original first arm, never renewed by approval, a phase event or a second wait.
Only the Test22 iOS sender opts in; other callers keep their existing behavior.
The closed preparation line accepts only fresh current-child stdout with its
private random nonce and exact case/cycle/step/role context. Diagnostic history,
foreign/stale/duplicate/out-of-order events and ended receipts cannot authorize a
phase. Existing global uniqueness, identity/window, fail-fast, cancel/drain and
restoration checks remain. Source/host checks do not certify native or whole
Test22 success: all39 original cycles,27 recoveries and12 direct Shows must rerun.

Test22 recovery with an iOS sender and Android approver prepares the current iOS
sender before starting Android, then requires Android readiness before the original
sender grant. One240000ms readiness window includes receiver startup and uses only
its remaining time, capped by the original ordinary600000ms iOS arm. The fulfilled
sender-ready proof belongs to that exact actor/cycle/step; later checks require the
same live owner rather than searching old bounded output again. An already-issued
receiver startup is settled before first failure propagates to the original owned
shutdown/drain. Other role pairs keep their original launch order. Android's
native incoming180000ms remains unchanged and still includes post-ready request
selection/transport; very slow post-ready work can still fail it. This changes
launch origins, not the39/27/12 scenario or exact value/seed+Copy/Close/actor results.
Host scheduling controls do not establish native or whole Test22 acceptance.

Test2 keeps its original18 cycles and final exact Web request removal before awaiting
iOS explicit completion and the unchanged natural Android instrumentation result.
Only its already-passed selected iOS suite permits bounded owned-wrapper shutdown;
Android summary text alone never substitutes for the original instrumentation result.


Prospective V5 Android supporting proof also requires the exact generated
`android-stripped-arm64` leaf under the existing stripped-native-libs environment
target. Capture and retained verification bind its raw bytes to every prepared
Android tuple's packaged/fresh arm64 provenance; all original three-ABI and child
triple checks remain mandatory. V3/V4 layouts stay unchanged. Older V5 seals
without this entry are rejected under the new strict recipe and require a genuine
new generation; they are never upgraded, rehashed or resealed. This generated
output support is not another Gradle build receipt.

### Prepared product copy storage

Product and supporting-product stash copies request Node COPYFILE_FICLONE as a copy-on-write hint, with ordinary-copy fallback on unsupported filesystems. Each destination remains independently writable; no hardlinks are used. Existing byte/tree/metadata, ownership, input, provenance and publication checks remain authoritative. Resource floors and full-copy reservations are unchanged: physical sharing and saved bytes are not guaranteed, and later writes may allocate additional blocks. This changes storage copying only, not build acceptance or historical replay/PASS authority.


## Persistent original-environment snapshot (TEST harness)

The normal range and existing Test51 preparation pass their already-selected
runDirectory to the original environment guard. The guard creates a fresh
`environment-snapshot-*` child under that canonical persistent run directory,
outside OS TMP and disjoint from every original snapshot target. For the range
this is `.artifacts/runs/<run>/environment-snapshot-*`; existing registration
preparation uses its own `test51-preparation` run child. No old backup is adopted
and missing/linked/foreign/overlapping roots cannot silently fall back to TMP.

The child is current-owned0700; its parent is validated but never chmodded.
Selected destination-volume budgeting preserves full logical-copy accounting,
no clone credit and the original12GiB remaining floor; normal16GiB/3GiB and
preparation/completion resource guards remain. Exact original names, modes,
link text, bytes, absence entries, repository fingerprints and private WeakMap
lifetime still govern capture/restore. Snapshot directory identity is checked
before own cleanup, including partial capture; a replacement is retained and
the original capture error is not masked. This is no new all-backup precheck or
repair algorithm.

After capture completes, only fully verified successful restoration removes
this guard's completed snapshot. Restore failures retain backup/evidence and
their original status. A failed partial capture before local transition cleans
only its same owned partial child, retaining the original error; an unknown
replacement remains intact. Protected older,
active/recovery/resumable/unknown siblings are never removed by name. Ordinary
successful-run pruning retains these children while the guard is active; its
empty log-directory cleanup uses empty-only rmdir, refusing a raced new file or
symlink. Existing obsolete-backup release rules remain separate.

Tests that construct the real guard use explicit fresh persistent destinations
from the TEST-only environment-test-fixture allocator; their original tiny
targets may remain in TMP. These isolated synthetic roots and teardown are not
working-baseline, SDK, F1 or UI evidence. Existing guard/provenance/lineage/error/
cancellation/resource assertions remain required.

Historical G66 remains restore-failed (whole24 passed,25 interrupted). A later
separately admitted working baseline is not a successful G66 restoration or
permission to resume/reseal it. Persistent storage addresses the observed TMP
cleanup exposure; it does not guarantee survival against every external
mutation. No system services, ages, schedules or timestamps are changed. Never
print configuration/backup payloads, credentials, Secrets, keys or shares.
