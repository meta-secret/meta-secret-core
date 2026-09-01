# Meta Secret E2E tests

The tests in this directory orchestrate the local server, visible Web browser, and platform simulators.

## Test #1

Install dependencies and run:

```bash
npm install
npm run test-1
```

The orchestrator builds a fresh local server image, starts it on port `3000`, starts the Web client on port `5173`, and opens a visible Chromium window. It creates the Vault and Secret, verifies Web `Show`, then runs the iOS and Android join flows. Web approves both requests.

Test #1 is a recovery-lifecycle regression scenario. It runs consecutive recovery cycles with iOS approval, Android approval, and alternating approvers. Each cycle verifies that both receiver devices receive the request, the selected device approves it, Web reveals the recovered secret, and the other receiver closes its alert after recovery completes. A short pause separates cycles to expose stale-claim, status-synchronization, and request-ordering defects.

The iOS simulator name is configured in `scenarios/test-1.json`. The test reuses that existing simulator: it does not clone devices and does not erase simulator content/settings. Before each iOS launch it boots the simulator if needed and uninstalls `org.metasecret.vault`.

Android is configured there too (`Pixel_4a` by default). The orchestrator reuses the existing AVD and starts it only when it is not already running. It never clones or wipes an AVD; before each Android run it uninstalls `metasecret.project.com`.

The browser stays open after success so the flow can be observed. Press `Ctrl+C` to stop the test and remove its server container.
