# Test #24 — iOS / Web / Android multi-secret lifecycle

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and all three Secrets |
| B | Web | Joins first; performs direct Show, sender, and approver roles |
| C | Android | Joins after the 12 direct-Show cycles; performs sender and approver roles |

Run locally from `meta-secret-core/e2eTest` through the normal range runner:

```sh
npm run test-range -- --from 24 --to 24 --run-id MS-TEST24-FRESH-001 --preserve-evidence
```

Use a fresh ASCII run ID for a new run (1–80 letters, digits, dots, underscores
or hyphens, beginning with a letter or digit); replace the example each time.
The normal runner creates the current run/attempt ownership and supplies the
required `E2E_RANGE_RUN`, `E2E_RUN_ID` and `E2E_TEST_ATTEMPT` context to its child.
The bare `test-24:run` forwarder does not create that qualified owner and is not
the standalone entry point for scoped Recover cycles13–39. Do not fabricate
the ownership environment by hand. Existing resume must use the recorded run ID
and pending-test/range rules rather than starting a different scope.

Secret1 is created before Web joins. iOS and Web each show it three times,
then iOS creates Secret2 and both devices show it three times. Android joins
after cycle 12. Secret1 and Secret2 each run all three sender roles three
times; iOS then creates Secret3, which repeats the same nine-cycle recovery
matrix. The complete scenario is 12 direct-show cycles plus 27 recovery
cycles.

The shared Test4 orchestrator recycles the Web renderer after cycles 21 and 30
while preserving its browser context, IndexedDB, and passkey. This bounds heap
growth without shortening the scenario or introducing fixed delays.

Recover cycles13–39 use the test-only [receiver phase](../shared/recovery-phase.md)
to start each receiver's existing180s incoming-request window after the current
sender has actually submitted the request. All three current participants must
first finish the original UI readiness checks; both receivers must still see the
current request before the sole selected approver can respond. Direct Shows1–12,
all39 original roles/repetitions, exact value/Copy/Hide assertions and numeric
timeouts remain unchanged. Phase tokens are harness metadata, never Core Claim
identities or authority to approve/reveal.

The Root coordinator enforces one stricter600s cycle cap beginning before actor
launch, shortened by the actual existing iOS selected-suite deadline. The badge
window has one origin at accepted current REQUEST_SENT; polls cannot extend it.
Abort closes new phase views and old already-allowed approval reads, and enters
the normal whole-harness shutdown. Killing Android's returned logcat child alone
does not stop its authoritative instrumentation actor.

Host regressions use `node --test shared/recovery-phase*.test.mjs` from e2eTest;
run the full ordinary shared suite with `node --test shared/*.test.mjs`.
Host/API checks are separate from owning native test compilation and full real
Core UI E2E. A test-helper source change requires fresh verified native products;
never reseal old binaries as the changed source. A full case24 result still
requires starting at the original beginning and completing all39 actions.
