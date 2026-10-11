# Test #49 — final declined-removal role rotation

| Role | Device | Responsibility |
| --- | --- | --- |
| A | iOS | Creates Vault + Secret1; approves both Joins; is the first target; declines Web's request; requests Web removal |
| B | Web | Joins first; creates Secret2; declines Android's request; requests Android removal; is the final target |
| C | Android | Joins second; creates Secret3; requests iOS removal; is the second target; declines iOS's request |

Run locally with `npm run test-49:run`.

The visible removal sequence is:

1. Android requests removal of iOS; Web terminally declines.
2. Web requests removal of Android; iOS terminally declines.
3. iOS requests removal of Web; Android terminally declines.

After every decline the pending request is gone, the exact three-member Vault
is preserved, and no stale recovery/removal UI remains. Each platform then
recovers all three Secrets once. This is 9 cycles per phase and 27 total.
Both receivers must expose the request before approval, the observer must
clear afterward, and the sender must reveal the exact value and close it.

The recovery matrix is the same balanced matrix as Tests #47–48: every
platform is sender, approver, and observer exactly nine times. All work is
local-only and bounded by visible/persisted readiness; there are no sleeps,
`slowMo`, reduced cycles, or direct Core business actions.
