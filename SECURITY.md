# SECURITY.md

## Principles

- **No secrets in logs:** Never log a Master Key, plaintext Secret, Key Share (Доля), encrypted Key Share, password, or recovery material in any build (debug or production). Use opaque ids and status/count fields only.
- **Local E2E diagnostics:** Core diagnostic sinks emit only when the compiled server environment is `Local`, including release native libraries used by local E2E. They emit nothing for `Remote`. JSON parsing failures must not log the input, exception message, or stack trace; report a payload-free error class instead.
- **Safe database filenames:** Mobile local databases use `meta-secret-db-<SHA-256(master_key)>.db`, where the digest is lowercase hexadecimal. The raw Master Key must never appear in a filename or path log.
- **Crypto changes are high-risk:** Require clear threat model, minimal diff, and tests. Avoid “quick” algorithm or parameter tweaks.
- **Operational files:** `master_key.json` and similar runtime secrets belong only in controlled environments—never commit them.

## Native approval retry evidence

Recover-Approve `notSubmitted` metadata is invocation evidence, never authority.
It may be emitted only after the first pre-sync failed before response production
and the same native owner validated complete pending receiver statuses and absent
exact local Recovery/Decline slots. Errors or uncertainty in that audit refuse
evidence while preserving the original operation error. An acknowledged response
may already have disappeared locally; source absence alone cannot prove that no
historical server outcome exists.

The final native approval path rechecks authoritative current state under the
same client-operation owner through source validation, response generation and
post-sync. Preserve client→device-sync lock order, existing authentication and
Core permissions. Clients require an exact original-intent/scope binding, fresh
post-return canonical evidence, a new explicit action and fresh authentication;
generic errors, stale callbacks and metadata from another owner cannot unlock
retry. Never automatically resubmit or rewrite a retained uncertain outcome as
success. See [the binding contract](docs/liveboard-bindings.md).

## Code review checklist (AI and humans)

- [ ] No `println!` / `tracing` of sensitive payloads.
- [ ] No client logger, `Logcat`, Xcode log, or crash/debug output contains a Master Key, Secret, or Key Share.
- [ ] Errors returned to clients do not leak internal paths or stack details in production builds (where applicable).
- [ ] New network surfaces validate inputs and sizes where relevant.
- [ ] Dependencies added for crypto/network are justified and pinned per project policy.

## FFI / mobile

- Treat exported functions as a **stable API** for the compose app. Breaking changes need versioning and coordinated release.

## Reporting

- Use the project’s normal issue tracker for security-sensitive reports as documented by maintainers.
