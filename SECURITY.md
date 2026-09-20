# SECURITY.md

## Principles

- **No secrets in logs:** Never log a Master Key, plaintext Secret, Key Share (Доля), encrypted Key Share, password, or recovery material in any build (debug or production). Use opaque ids and status/count fields only.
- **Safe database filenames:** Mobile local databases use `meta-secret-db-<SHA-256(master_key)>.db`, where the digest is lowercase hexadecimal. The raw Master Key must never appear in a filename or path log.
- **Crypto changes are high-risk:** Require clear threat model, minimal diff, and tests. Avoid “quick” algorithm or parameter tweaks.
- **Operational files:** `master_key.json` and similar runtime secrets belong only in controlled environments—never commit them.

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
