# Test #35 — iOS Secret2 creator, iOS-approved Web Join

This is a visible local UI E2E scenario. It reuses the Test #4
Android-secondary-creator lifecycle and does not add a parallel runner, fixed
sleeps, `slowMo`, direct Core calls, or weakened assertions.

## Setup

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Android | Creates the Vault and Secret1 |
| B | iOS | Joins first, creates Secret2, and approves Web joining |
| C | Web | Joins second and creates Secret3 after cycle 18 |

1. Android creates the Vault and Secret1 through the visible Compose UI.
2. iOS submits a Join request; Android approves it. iOS verifies Secret1.
3. iOS creates Secret2 and Android observes the redistributed row.
4. Web submits the second Join request. iOS is the only approver; Android does
   not approve this request. Web observes Secret1 and Secret2 after joining.

## Recovery matrix

Cycles 1–18 recover Secret1 and Secret2. Each Secret has three repetitions for
each sender platform. After completed cycle 18, Web creates Secret3. Cycles
19–27 recover Secret3 with the same three-platform matrix.

The scenario has exactly 27 recovery cycles and zero direct-Show cycles. Every
platform acts exactly nine times as sender, nine times as approver, and nine
times as observer. Before each Approve, both receivers must expose the current
request badge/open action. The observer must lose its request after the
selected Approve, and the sender must reveal the exact synthetic value and
close the reveal state before the next cycle.

## Run and diagnostics

Run locally with `npm run test-35:run`. The shared Test #4 lifecycle records
local mode, server/native tails, browser diagnostics, marker timeline,
screenshots on failure, and cleanup status. It must never print Secret values,
keys, or raw Shares. To keep the 27-cycle run within the workstation memory
budget, the iOS target is built once with `build-for-testing`; every visible
iOS action then uses `test-without-building` and waits for the preceding
`xcodebuild` process to close before starting the next action.
