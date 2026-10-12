# Test50 — Vault-wide Secret deletion

Run the complete local scenario through the existing range coordinator:

```sh
cd /Users/dmitrykuklin/Documents/Projects/MetaSecret/meta-secret-core/e2eTest
npm run test-range -- --from 50 --to 50 --run-id ms96-delete --preserve-evidence
```

F1 only: `npm run test-50:f1`. This runs pure contract/metadata tests and a
loopback-only proxy unit test, without Docker, browsers or mobile runtimes.

## Matrix

| Blocks | Members | Variants | Repetitions | Cycles |
|---|---|---|---:|---:|
| one-web/android/ios/cli | One named platform | Local confirmation | 3 each | 12 |
| two-1 | Web / Android | Each requests; the other is offline | 3 each | 6 |
| two-2 | CLI / iOS | Each requests; the other is offline | 3 each | 6 |
| three-1 | Web / Android / iOS | Rotate requester; next member approves or declines | 3 each | 18 |
| three-2 | CLI / Android / iOS | Same rotation | 3 each | 18 |

All four platforms request, approve, decline and catch up offline. The complete
plan contains **8 sequential blocks / 60 cycles** and never more than three
Vault members. Every block has a distinct Vault/run ID and fresh app/server
state; within an offline cycle the existing DB/preferences are preserved.

Each cycle checks cancellation, exact current Pass ID, the applicable policy,
persisted terminal UUID and the rendered result on every peer. At one/two
members every actor verifies local Show and native own-refresh preservation.
Accepted deletion recreates the same display name with a different synthetic
value and requires a fresh opaque ID. CLI/WASM stale-ID reads must reject.
Three-member cases prepare both receivers before the request and release only
the declared first responder; all peers must lose the incoming/pending UI.
CLI late opposite decisions, where eligible, submit the original UUID and must
leave the winning outcome unchanged.

Native two-member offline checks first open Show, isolate the exact native
transport including its existing SSE connection, and verify the opaque
read/copy blocker. The app is then terminated **before deletion**, preserving
its sandbox. After the other member deletes, its cold launch waits behind a
real HTTP response hold. Protected Splash or the opaque overlay must prevent
cached reveal. Both the native ready marker and a held canonical HTTP request
are required before release; the real startup path applies the tombstone.
This sequence repeats three times for each native offline variant.

Web delays one real local reveal result, disconnects its transport, and requires
actual reconnect/SSE to remove the row before releasing that late result. The
late result must not render. A reload then checks persisted catch-up. CLI
processes exit between commands, and their first reconnect command must return
the cleaned canonical state.

## Lifecycle and evidence

Test4 retains local-environment snapshot/restore, visible clients, native
installation/readiness, provenance, process ownership, redaction and cleanup.
Test50 builds Android APKs and the CLI before business setup; subsequent blocks
reuse fingerprint-verified artifacts. iOS uses one `build-for-testing` and
`test-without-building`. Runtime selectors remain Pixel_4a / iPhone 17.

`markers.mjs` produces the exact native ledger: **617 producer events**, of
which **537 have business consumers** and **80 are diagnostic-only**. The additional 88 iOS routing-gate events occur before each owned XCTest actor launches the app. Business
step names include run/block/cycle/platform/action. Generic legacy setup
markers are scoped to their fresh owned process and block log.

The first native suite failure immediately rejects the business run. Test4's
shared shutdown barrier cancels sibling waits/processes, records diagnostics
once, then restores the environment. The intentionally terminated offline
actor is explicitly identified and its process result consumed before startup.

Canonical metadata diagnostics observe existing `/meta_request` responses;
they never issue an additional passive synchronization RPC. Only Secret
names/IDs, terminal request UUID/status, tombstone IDs and canonical revision
survive the whitelist. The highest event revision wins, even for unordered
history or late HTTP responses. Raw responses, users, keys, ciphertext and
plaintext are never emitted or persisted. Missing/unknown metadata fails.

F4 must still establish actual native compilation, Copy accessibility mapping,
full-window overlay stacking, startup holds, and all declared terminal UI
assertions. F1 does not claim those runtime results.
