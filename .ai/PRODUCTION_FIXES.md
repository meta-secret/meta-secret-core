# Cumulative Production Fixes

Scope: production-code corrections only. Test-harness changes and logging-only
changes are intentionally excluded.

**Last updated:** 2026-09-19

## Cumulative production corrections

1. **The primary secret action did not reflect the real recovery state.**

   **Correction:** the UI shows `Show` when the secret can already be opened
   and `Recover` only when a new recovery request is required.

2. **Incoming recovery requests were mixed with the sender's primary action.**

   **Correction:** incoming requests have their own `Open request` action;
   the second action remains `Recover`/`Show` for the local device. The UI also
   exposes the request count and the requesting device type.

3. **Repeated recovery taps could create duplicate claims.**

   **Correction:** an existing pending or accepted claim for the same sender,
   secret, and vault is reused instead of creating another recovery claim.

4. **Recovery claim lookup depended on HashMap iteration order.**

   **Correction:** claim selection is deterministic and sender-scoped; multiple
   accepted/active claims are treated as an explicit ambiguity instead of an
   arbitrary claim being selected.

5. **A newly joined device did not reliably receive shares from every existing
   sender device.**

   **Correction:** membership synchronization detects newly accepted members
   and each existing sender redistributes the secrets it owns exactly once.

6. **Concurrent mobile FFI calls could consume another operation's response.**

   **Correction:** mobile UniFFI operations are serialized per process so state
   refresh, claim lookup, approval, and recovery cannot cross-consume replies.

7. **Concurrent server data requests shared one response channel.**

   **Correction:** each server request carries its own response channel, so
   concurrent callers always receive their own response.

8. **Recovery decisions were not terminal.**

   A stale claim snapshot could overwrite an already accepted or declined
   receiver decision and return it to `Pending`.

   **Correction:** receiver decisions are monotonic. `Pending` can transition
   to `Declined` or `Sent`/`Delivered`, and stale snapshots cannot restore a
   terminal decision.

9. **Approve/decline races had no deterministic first-response rule.**

   **Correction:** the first receiver decision processed by the server is
   authoritative. A first `Decline` terminalizes remaining pending receivers
   and blocks reveal. A first `Approve` enables recovery; a later `Decline` is
   ignored.

10. **Offline recovery decisions and mobile completion state could be reordered
    or silently fail.**

    **Correction:** local recovery workflow events are synchronized before a
    server snapshot, and mobile approve/decline/completion synchronization
    errors are propagated to callers.
