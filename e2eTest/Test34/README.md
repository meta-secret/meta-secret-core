# Test #34 — Android Secret2 creator, Android-approved iOS Join

This is a visible local UI E2E scenario. It reuses the Test #4 Web-secondary-
creator lifecycle and does not add a parallel runner, fixed sleeps, `slowMo`,
direct Core calls, or weakened assertions.

## Setup

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates the Vault and Secret1 |
| B | Android | Joins first, creates Secret2, and approves iOS joining |
| C | iOS | Joins second and creates Secret3 after cycle 12 |

1. Web creates the Vault and Secret1 through the visible UI.
2. Android joins and observes Secret1.
3. Android creates Secret2 through its visible UI; Web and Android observe both
   existing Secret rows.
4. iOS submits the second Join request. Android is the only approver; Web does
   not approve this request. iOS observes Secret1 and Secret2 after joining.

## Recovery matrix

Cycles 1–12 recover Secret1 and Secret2. Each Secret has two repetitions for
each sender platform. After completed cycle 12, iOS creates Secret3. Cycles
13–18 recover Secret3 with the same three-platform matrix.

The scenario has exactly 18 recovery cycles and zero direct-Show cycles. Every
platform acts exactly six times as sender, six times as approver, and six times
as observer. Before each Approve, both receivers must expose the current
request badge/open action. The observer must lose its request after the
selected Approve, and the sender must reveal the exact synthetic value and
close the reveal state before the next cycle.

## Run and diagnostics

Run locally with `npm run test-34:run`. The shared Test #4 lifecycle records the
local mode, server/native tails, browser diagnostics, marker timeline,
screenshots on failure, and cleanup status. It must never print Secret values,
keys, or raw Shares.


## Scoped recovery orchestration

Exact `test34:web-secondary-creator-lifecycle` uses the actual current source descriptor and
independent expected12 before raw metadata. Its iOS sender uses the distinct
`recoveryShowPhase` procedure at the original input-return/already-visible
boundary. Same ordinary600/currentD clips prep180/value45/close60; no business
or security rule changes. Launch through normal range owner, for example
`npm run test-range -- --from 34 --to 34 --run-id <fresh-ascii-owner> --preserve-evidence`. Original
actions/roles/repetitions/value/Copy/Hide/results stay mandatory. Host controls
are not whole UI or SDK/provenance acceptance.
