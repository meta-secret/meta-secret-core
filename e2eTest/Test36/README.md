# Test #36 — Web Secret2 creator, Web-approved Android Join

This is a visible local UI E2E scenario. It reuses the Test #4 iOS distributed-
creator lifecycle and does not add a parallel runner, fixed sleeps, `slowMo`,
direct Core business actions, or weakened assertions.

## Setup

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and Secret1 |
| B | Web | Joins first, creates Secret2, and solely approves Android joining |
| C | Android | Joins second and creates Secret3 after completed cycle 18 |

1. iOS creates the Vault and Secret1 through the visible UI.
2. Web submits the first Join request; iOS approves it through the visible UI.
3. Web creates Secret2; iOS observes its redistributed row.
4. Android submits the second Join request. Web is the only approver; the
   orchestrator does not launch an iOS approval action. Android observes
   Secret1 and Secret2 before recovery begins.

## Recovery matrix

Cycles 1–18 recover Secret1 and Secret2, with three repetitions for every
sender platform per Secret. After completed cycle 18, Android creates Secret3
through the visible Compose UI. Web and iOS observe its row before cycles
19–27 recover Secret3 with the same three-platform matrix.

The scenario has exactly 27 recovery cycles and zero direct-Show cycles. Every
platform acts exactly nine times as sender, nine times as approver, and nine
times as observer. Before each Approve, both receivers must expose the current
request badge/open action and open the exact recovery alert through visible UI.
The selected approver keeps its alert open. The observer closes only its local
alert through the visible backdrop/Escape path, without approving or declining,
and must still expose the unchanged badge/open action. This per-receiver alert
evidence is completed before the selected Approve; the two alerts do not need
to remain open concurrently. After Approve, the observer must lose its request,
and the sender must reveal the exact synthetic value and close the reveal state
before the next cycle. Web close is strict: a missing close control or a still-
visible exact value fails the cycle.

## Run and diagnostics

Run locally with `npm run test-36:run`. The browser, DeviceHub/Simulator, and
Android emulator remain visible. Before starting services, the shared harness
snapshots the exact Core/Compose dirty-state fingerprint, all environment
config bytes, prior mode, local ports, scenario-container ownership, and the
complete contents/absence and hashes of the iOS libraries, Android `jniLibs`,
and generated mobile UniFFI directories. The native backup lives in a unique
temporary directory. The harness switches through `task set-env -- local`,
owns only services proven free, and restores the exact prior mode/config,
native artifacts, and repository state after its owned process tree has
actually terminated. All cleanup callers share and await one Promise; signals
and the generic failure path do not exit before restore finishes. The backup
is removed only after byte/hash and working-tree verification; any mismatch or
remaining descendant is a blocker.

Generic failure diagnostics record Web, iOS, and Android capture status plus a
JSON path manifest under `e2eTest/.artifacts/`. Web masks exact synthetic
values. Native capture never snapshots the MetaSecret UI: iOS terminates the
target and foregrounds Settings, while Android force-stops the target,
foregrounds Settings, and verifies the focused package first. The manifest
records the sanitization method; a failed sanitization produces no native
screenshot, and any stale same-name file is removed before the attempt.

Every native step emits `*_SENDER_UI_READY_<cycle>_1` or
`*_RECEIVER_UI_READY_<cycle>_1` only after the secret row is rendered. Sender
readiness first consumes/closes any stale Show state, then requires the exact
`Recover` action and no reveal/dialog/request/alert state. Receiver readiness
strictly requires no badge, open action, alert, or processing state; a request
that is already actionable is never accepted as pre-request readiness. The
orchestrator waits these exact markers before allowing the sender to create the
Claim; `*_STEP_CONFIG` is diagnostic only. Before either receiver emits its
alert-visible marker, Android and iOS assert that the alert contains the exact
scenario `secretName`.

The same pre-request contract is enforced on Web without a native marker: a
Web sender visibly consumes/closes stale `Show`, reaches an exact `Recover`,
and has no dialog/value/request/alert state; a Web receiver must have no
badge/open/alert state. An already-actionable request therefore cannot satisfy
any platform's readiness barrier. The shared barrier is wired before sender
release/Claim creation in every lifecycle that launches the clean-only
`handleRecoveryStep` hooks, including the Test34 Web-secondary and Test35
Android-secondary rotations. Their sender, approver, and observer are all
ready before release. When `verifyBothReceiverAlerts` is enabled, the flag and
alert-visible/dismiss semantics are propagated consistently; when false, the
existing badge/open flow is preserved after the readiness barrier.

Watched command arguments, launch environment, process tails, and browser logs
redact exact Secret values and Secret/Master/Private/Key-Share fields,
including quoted JSON camelCase and snake_case key variants. Dedicated Node
fixtures cover these JSON, scalar, argument, and environment cases. Watched
stdout/stderr use separate bounded stateful redactors: complete records are
redacted before emission, while an over-limit unfinished record is replaced
fail-closed and its continuation is quarantined through the next newline.
Thus a long quoted value cannot leak when split across arbitrary chunks.
Separated CLI keys (`--private-key value`, `master_key value`, `keyShare
value`, and Android `-e key value`) and equivalent environment keys redact the
following value as well.

The harness builds the iOS test bundle once with
`build-for-testing`, then uses `test-without-building` for each iOS step to
bound memory use. It must never print Secret values, keys, or raw Shares.
