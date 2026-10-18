# Test51 — Orbit registration only

Owner-approved authoring/run: 2026-10-18. One pass per variant, clean local
server DB and MetaSecret identity boundaries. No Secret/recovery scenarios.
Run20261018135952 separately authorizes only the Declined→Back cleanup repair.
Production is frozen during runtime: diagnose and obtain scope before any other
production repair. Scenario, harness and native test-hook repairs are allowed.

## Expanded matrix

`plan.mjs` is the executable matrix: **45 cases / 479 consumed completed-action
markers / one repetition**. The 45 is the expansion of the owner's platform,
provider and outcome requirements, not a separately requested repetition count.

| Group | Platforms | Providers/outcomes | Cases |
|---|---|---|---:|
| Create | Web, iOS, Android | manual, Apple, Google |9|
| Online Join | Web, iOS, Android requester; another platform approves | each provider × accepted, declined→reset01→different Vault accepted, Pending Back→01→different Vault accepted |27|
| QR Join | Web phone/tablet, iOS, Android scanner; another platform shows QR | accepted, declined→retry, Back→retry |9|

Order: Create first, then all accepted Join roles, then decline/retry, Back/retry,
then QR. Independent variants start with a new server container, app sandbox
reset and browser context. Native runtimes remain booted and are reused.
There is no reload/relaunch of a requesting client between Request and its
terminal decision/retry. Canonical candidate Device names come from the actual
pending row; runtime aliases never become Device identities.

For a Web requester, iOS owns the first Vault and Android the retry Vault.
For iOS, Android owns the first Vault and Web the retry Vault. For Android,
Web owns the first Vault and iOS the retry Vault. Both Vaults are created
before the requester starts. Responders signal READY before Request; their
decision gate opens only after the requester reports canonical Pending.

Synthetic fixture emails are deterministic37-character names (`v` +24 SHA256
hex characters + `@e2e.invalid`) derived from the JSON tuple of full run ID,
exact case ID and primary/retry stage. They remain unique across cases, stages
and runs without embedding the full diagnostic run name in a legacy Device
row. Full run IDs still identify DB/artifact scopes and all completed-action
markers. `cases.json` is regenerated from `buildRegistrationPlan('registration')`.
Real provider account/relay email overrides are preserved exactly.

The r2 legacy iOS88dp pending-row layout clipped candidate information when
synthetic Vault emails wrapped (observed78/81-character fixtures). Shortening
test data is not a fix or acceptance claim for long-name production layout.
Candidate Device names, exact canonical comparisons, visibility assertions and
Swift wait conditions remain unchanged; the legacy layout issue is separate.

All provider cases remain in the matrix. A real provider fixture requires both
`E2E_REAL_<WEB|IOS|ANDROID>_<APPLE|GOOGLE>_READY=1` and the exact corresponding
`..._EMAIL` value (including Apple relay). The fixture name is that account's
email; the server DB is reset per case. No account callback is faked. Android
Apple is unsupported by current production and stays Blocked. Real account
consent uses the OS/provider UI and may require supervised user interaction.

QR **generation** is asserted through actual Devices→Add device after Create
on each platform, including decoding the rendered QR and exact Vault-name
comparison. QR **physical scanning** remains an explicit manual/device
prerequisite; software decoder checks never count as physical-camera UI E2E.
Desktop Web may show QR, but its registration scanner stays disabled.

## Gates and commands

```sh
npm run test-51:f1
node Test51/orchestrator.mjs --validate-only
npm run test-51
# A focused diagnostic is not full-matrix acceptance:
npm run test-51 -- --case=create-web-manual
# Focused Balanced registration gate: one preparation/build/restore for three existing cases.
npm run test-51 -- --cases=join-web-manual-accepted,join-ios-manual-accepted,join-android-manual-accepted
```

The initial Test51 navigation waits for the exact configured loopback document
commit, rejects HTTP errors/unexpected URL changes, then waits for actual auth
or Orbit UI. Existing Passkey and authenticated entry gates still run afterward.
This avoids treating unrelated DOMContentLoaded completion as application
readiness; it does not bypass auth, refresh a requester or alter Tests1–50.

A separately executed root prerequisite is
`node Test51/web-navigation.browser-check.mjs`: controlled local rendered UI
with a held defer script proves legacy DCL timeout and helper readiness before
release. It also rejects500-with-UI, missing UI, redirect and transport failure.
It is not in F1 and is not registration/business acceptance.

`--from-case=join-android-manual-declined-retry` selects canonical25–45 (21
cases) as a partial new run. It is mutually exclusive with `--case` and
`--first-case`. Full requested45/marker plan479 remain unchanged; the result
manifest records selectedCases21 and businessAcceptance=false. No prior result
is adopted or converted to Pass. R5 and the suffix retain separate evidence;
root may reconcile unique outcomes only after actual same-production proof.

`--cases=<id>,<id>` selects only existing canonical case objects, in the supplied
order, within one existing `local-run.mjs` preparation/snapshot/restore scope.
It rejects empty IDs, duplicates, unknown IDs and combination with any other
selector. The approved Balanced gate above runs three cases once each with
24 consumed markers: iOS creates/approves for Web, Android for iOS, and Web
for Android. Each creator asserts AVAILABLE (Create enabled, Join disabled);
each requester asserts EXISTS (Create disabled, Join enabled) before canonical
Pending, approval and Main. It uses manual synthetic names, existing rendered
QR decoding, no real accounts/physical cameras and no Secret/recovery cases.
The full 45-case / 479-marker plan stays unchanged. The manifest truthfully
records selectedCases3 and businessAcceptance=false; actual focused 3/3 and
24/24 success is a separate task result, never full-matrix acceptance.

`local-run.mjs` owns the existing `createBatchEnvironmentGuard` snapshot and
restore, official local build checkpoint and one-time app/test package builds.
It passes verified-local/artifact handoff to the existing Test4 lifecycle.
Test4's registration-only branch executes before the historical Secret setup.
Web-only Create diagnostics do not require native SDKs, runtime inventory,
checkpoints or app builds. From a remote environment the same guard invokes
`task set-env -- local --selectors-only` once, preserving all four selectors
and original artifacts for exact final restoration. Default `task set-env`
still rebuilds mobile libraries. Web preparation builds/proves WASM and server,
rejects source/output drift and strips inherited native prebuilt/readiness
claims before the existing Web-scoped provenance handoff. A Web JOIN includes
native approvers and retains the full native/mixed preparation path.
Web uses free port5174;
the existing browser at5173 is not adopted or stopped. Native cases require
the parent's completed F2 handoff before any clients start. No native rebuild
occurs inside a business case.

F1 validates the full matrix, exact unique marker ledger, native selectors,
Node syntax, truthful prerequisites, no Secret fixture, helper regressions,
fixture/responder readiness isolation and shared first-failure cancellation.
F2 proves local endpoints and artifact provenance; F3 proves installed actors;
F4 runs actual Core-backed UI actions. Unit/F1 success is never F4 success.

Every active actor participates in one failure/cancellation barrier. A sibling
failure interrupts another actor's pending marker wait. Completed native actor
suite results are collected before teardown. The old generic range defaults
remain unchanged; a registration run selects only Test51.

## Pending and Declined cleanup — scope revision2

The owner-approved revision for run20261018135952 applies after Core reports
Pending or Declined and the requester presses Back: clear the unfinished local
registration, verify Local with no Vault and a fresh public device identity,
and return to initial01. Declined remains visible on04 until Back; Pending
waits without a timeout until Back or a decision. Neither cleanup claims to
revoke the old server-side request. Other Back destinations are unchanged.

Both nonaccepted outcomes emit **RESET01** only after real cleanup proof. iOS
first emits RESET_UI01 and waits for the host zero-count proof. Restoring the
four Pending iOS RESET_UI01 markers yields **479 markers /45 cases /one
repetition**; the12 currently available manual cases consume119 markers.
Historical reports retain their original expectations and results. After this
production revision a fresh full45-case run is required. Root may reorder all45
once with `--first-case=join-ios-manual-back-retry`; this is not a suffix or a
single-case acceptance claim.

Web captures the exact expected canonical Pending/Declined Vault and public
device ID before Back, requiring persisted credential records. Its read-only
cleanup oracle requires Core Local/empty name, a different nonempty public ID,
zero UserCreds in all four known IndexedDB stores, exactly one new DeviceCreds
bootstrap record in `meta-secret`, and no foreign DeviceCreds. This fresh Local
bootstrap is expected; the old attempt must be absent. Unknown inventory,
schema, identity or counts fail closed. No private values are read or logged.

For BOTH iOS nonaccepted outcomes the host first proves the exact scoped key
exists, then opens `registration-<id>-cleanup-presence-verified`. The native
requester cannot press Back before that gate. After RESET_UI01, exact scoped
zero-count proof opens `registration-<id>-reset-verified`, allowing RESET01.
Pending back-retry never opens the primary approval gate; its unused responder
is canceled after the retry completes. Android checks the injected KeyChain's
presence Boolean before Back and absence afterward. Global browser PasskeyAuth,
verified iOS namespace/hashed lookup guards and all later retry assertions stay
unchanged.

Run output/result manifests preserve passed, blocked, failed and not-run
cases. A requested provider/camera prerequisite is never silently skipped as
Pass. The queue pauses at a decisive failure for diagnosis; production changes
need the owner's separate approval.

Additional agreed smoke coverage (network Create error and three-second
return, persistent Join network error, Pending/Declined resume and phone/tablet
orientation captures) is tracked separately from these45 variants. It must
not be reported as executed merely because the matrix/F1 is complete.
