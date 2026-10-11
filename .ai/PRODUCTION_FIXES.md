# Cumulative Production Fixes

Scope: production-code corrections only. Test-harness changes and logging-only
changes are intentionally excluded.

**Last updated:** 2026-09-25

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

11. **Joining a new device left an in-flight recovery claim tied to the old
    membership set.**

    **Correction:** when a new member is accepted, the server terminalizes
    every still-pending receiver of an existing `Recover` claim as `Declined`
    and publishes the claims invalidation. The new member receives the
    redistributed `Split` claim and never receives the stale recovery request;
    existing `Sent`/`Delivered` decisions are preserved.

12. **A Web recovery decision made while offline was not flushed reliably at
    reconnect.**

    **Correction:** the Web UI now performs an explicit, idempotent recovery
    sync when the browser returns online and then refreshes application state.
    The reconnect sync performs a second pass to cover the background sync
    worker racing the reconnect boundary, so the canonical claim status is
    visible without changing the normal mobile refresh path.

13. **The supported three-device sharing scheme had no enforced upper bound.**

    **Correction:** Core rejects every fourth-device join at the vault boundary
    with a single `maximum of 3 devices` error shared by Web, Mobile, and CLI.
    The check is applied both when a join request is accepted and when the
    resulting membership update is written, so clients cannot bypass it.

14. **After a three-device redistribution, a sender could retain other devices'
    Encrypted Key Shares locally.**

    **Correction:** each device keeps only its own Encrypted Key Share. Outbound
    Encrypted Key Share workflows for other devices are deleted after successful
    upload (and stale copies are cleaned during sync), so the sender cannot
    reconstruct a three-device secret alone from foreign shares.

15. **Server-side sync errors could be reported as successful client state.**

    **Correction:** sync paths now inspect `DataSyncResponse::Error` and
    propagate the error through Core/FFI instead of continuing as if the write
    had succeeded.

16. **Fourth-device rejection was a silent/panicking failure at the clients.**

    **Correction:** WASM registration propagates the Core error instead of
    unwrapping it, and Web/Mobile render a clear retryable message with a
    reset/new-vault action.

17. **A completed old recovery claim could close the alert for a newer claim.**

    **Correction:** recovery-alert dismissal is bound to the completed Claim
    ID. Mobile clients close the dialog only when that Claim ID matches the
    request currently open in the UI, so an old `DONE` state cannot dismiss a
    newer recovery request.

18. **Concurrent state refreshes could resend the same signed sync action and
    leave a client on stale recovery state.**

    **Correction:** Core serializes the complete sync transaction per client —
    from reading the server tail through uploading local events and receiving
    vault/claim updates — so a second caller observes the fresh sequence
    instead of replaying the previous action. Mobile Splash also coalesces
    duplicate biometric-success events into one authentication and navigation
    flow, preventing competing startup refreshes.

19. **The sync serialization lock could panic in WebAssembly when two sync
    callers contended for it.**

    **Correction:** Core uses the WASM-safe `async-lock` mutex for per-client
    sync and in-memory database serialization. Its contested path does not
    require `std::time::Instant`, so concurrent Web approval and SSE state
    refresh no longer crash with `time not implemented` or a nested `RefCell`
    borrow panic.

20. **A completed `3 → 2` device removal could leave the remaining devices
    with unusable old `k=2` Key Shares.**

    **Correction:** the approving device now completes reverse resharing as
    part of accepting the removal. It combines its own Key Share with the
    initiator's Encrypted Key Share from the signed request, reconstructs the
    Secret only in memory, and distributes fresh `k=1` Key Shares to the two
    remaining devices. The removed device receives no new Key Share, and each
    remaining device retains only its own new Key Share.

21. **An incoming removal-request row could ignore taps on iOS.**

    **Correction:** a pending removal target is rendered as a plain clickable
    row instead of being wrapped by the swipe-to-delete gesture container.
    This makes Approve/Decline dialog opening deterministic on iOS and removes
    the irrelevant delete swipe from an incoming request, while ordinary
    removable member rows retain their swipe action.

22. **A `2 → 1` removal could strand Secrets owned by the removed device.**

    **Correction:** after the locally confirmed removal, Core makes the sole
    remaining device the canonical sender for every surviving `Split` claim and
    clears removed receivers/statuses. The existing local `k=1` Key Share stays
    unchanged. A later Join therefore redistributes every Secret, regardless of
    which removed device originally created it.

23. **Declining a pending Device Removal only closed the client dialog.**

    **Correction:** Core now records a signed terminal `Declined` response for
    the exact removal request. It consumes that request while preserving Vault
    membership and all Key Shares, performs no resharing, and does not
    invalidate active Recovery claims. The server enforces first-response-wins,
    so a late or duplicate `Accepted` response cannot remove the Device.
