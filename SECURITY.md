# SECURITY.md

## Principles

- **No secrets in logs:** Never log passwords, raw key material, full shares, or recovery material. Use opaque ids or redacted previews only when necessary for debugging.
- **Crypto changes are high-risk:** Require clear threat model, minimal diff, and tests. Avoid “quick” algorithm or parameter tweaks.
- **Operational files:** `master_key.json` and similar runtime secrets belong only in controlled environments—never commit them.

## Code review checklist (AI and humans)

- [ ] No `println!` / `tracing` of sensitive payloads.
- [ ] Errors returned to clients do not leak internal paths or stack details in production builds (where applicable).
- [ ] New network surfaces validate inputs and sizes where relevant.
- [ ] Dependencies added for crypto/network are justified and pinned per project policy.

## FFI / mobile

- Treat exported functions as a **stable API** for the compose app. Breaking changes need versioning and coordinated release.

## Reporting

- Use the project’s normal issue tracker for security-sensitive reports as documented by maintainers.
