# Test #33 — iOS owner, Web-only second Join approval

This is a visible local UI E2E scenario. It reuses the Test #4 lifecycle and
does not add a parallel runner, fixed sleeps, `slowMo`, or direct Core calls.

## Setup boundary

1. iOS creates the Vault and `Secret1` through the visible UI.
2. Web joins and iOS approves that first Join.
3. iOS creates `Secret2`; Web must observe the row before Android joins.
4. Android submits its Join. Web is the only approver: the orchestrator never
   launches an iOS approval step for this second Join. Android must observe
   both `Secret1` and `Secret2` after the Join.

Each setup action emits a completed-state marker after its row/state is
observable. The Web approval is the semantic boundary for Android's Join.

## Recovery matrix

Cycles 1–18 recover `Secret1` and `Secret2`: each Secret has three repetitions
for each sender platform. Cycles 19–27 recover `Secret3` with the same matrix.
There are exactly 27 recovery cycles, no direct-Show cycles, and each platform
acts exactly nine times as sender, nine times as approver, and nine times as
observer.

For every cycle the runner must, in order:

1. observe the request badge and open action on both receivers;
2. release only the selected approver;
3. verify the observer's badge/open action disappears;
4. reveal the expected synthetic value on the sender;
5. close the reveal dialog and verify the closed state before the next cycle.

Immediately after completed cycle 18, iOS creates `Secret3`. Web, Android, and
iOS must observe its row before cycle 19 begins. Names and values are synthetic;
the runner must never print a Secret value, key, or raw share.

## Run and diagnostics

Run locally with `npm run test-33:run` (or
`node Test33/orchestrator.mjs scenario.json --exit-on-success`). The Test #4
orchestrator records the local mode, server/native process tails, browser
console/page errors, E2E marker timeline, screenshots on failure, and cleanup
status. It must be invoked through the local UI E2E command so the environment
is restored in `finally`.
