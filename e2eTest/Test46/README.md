# Test #46 — removal, re-Join, redistribution, and recovery

| Role | Device | Responsibility |
| --- | --- | --- |
| A | Web | Creates Vault + Secret1; is removed; re-Joins through Android approval; directly Shows all Secrets; approves iOS re-Join; sends/approves/observes recovery |
| B | Android | Joins first; creates Secret2; approves initial iOS Join; requests Web removal; locally removes iOS; approves Web re-Join; directly Shows all Secrets; sends/approves/observes recovery |
| C | iOS | Joins through Android approval; creates Secret3; approves Web removal; is removed; re-Joins through Web approval; sends/approves/observes recovery |

Run locally through the guarded visible runner with `npm run test-46:run`.

The first membership phase creates one Secret on each platform. Android then
requests removal of Web and iOS approves the exact pending request. At the
two-member boundary Android locally removes iOS, leaving Android as the sole
Device. Both removed clients must visibly reset with no stale Vault, Join,
removal, recovery, or Secret UI.

Web submits a new Join request and Android approves it. At the restored
two-device boundary, Web and Android each directly Show, verify the exact
value of, and close all three Secrets. iOS then submits a new Join request and
Web approves it, exercising a fresh 2→3 redistribution after both removed
Devices have returned through normal UI flows.

The final matrix contains exactly 27 recovery cycles: every platform sends a
recovery request for every Secret three times. The approver alternates
deterministically so Web, Android, and iOS each act nine times as sender, nine
times as approver, and nine times as observer. Before approval both receivers
must expose the Claim; after the first approval the observer's alert/badge
must disappear; the sender must reach Show, reveal the exact value, and close
the visible dialog.

`Pixel_4a` and `iPhone 17` are launch selectors only. Canonical Device names
are discovered from authoritative Web rows before removal and again after
each re-Join. Every wait is bounded by persisted UI/Core state or a marker.
There are no sleeps, `slowMo`, reduced cycles, remote services, or direct Core
business actions.
