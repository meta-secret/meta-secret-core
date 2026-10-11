# Test #24 — iOS / Web / Android multi-secret lifecycle

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates the Vault and all three Secrets |
| B | Web | Joins first; performs direct Show, sender, and approver roles |
| C | Android | Joins after the 12 direct-Show cycles; performs sender and approver roles |

Run locally with `npm run test-24:run`.

Secret1 is created before Web joins. iOS and Web each show it three times,
then iOS creates Secret2 and both devices show it three times. Android joins
after cycle 12. Secret1 and Secret2 each run all three sender roles three
times; iOS then creates Secret3, which repeats the same nine-cycle recovery
matrix. The complete scenario is 12 direct-show cycles plus 27 recovery
cycles.

The shared Test4 orchestrator recycles the Web renderer after cycles 21 and 30
while preserving its browser context, IndexedDB, and passkey. This bounds heap
growth without shortening the scenario or introducing fixed delays.
