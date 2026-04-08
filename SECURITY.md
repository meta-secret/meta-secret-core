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

## Meta server HTTP and WebSocket

- **`POST /meta_request`** and **`GET /meta_ws`** are part of the same sync surface. Neither implements application-level bearer tokens in the reference server; deploy behind a trusted network, VPN, or reverse proxy as needed.
- **`/meta_ws` Origin policy:** set **`META_WS_ALLOWED_ORIGINS`** to a comma-separated list of allowed `Origin` values (e.g. `http://127.0.0.1:5173,https://app.example.com`). When unset or empty, the server **does not** enforce Origin (development default). Browsers send `Origin` on WebSocket upgrades; many **native** clients omit it. Use **`META_WS_ALLOW_NO_ORIGIN`** (`true`/`false`, default `true`) to control whether connections **without** an `Origin` header are accepted when an allowlist is configured. For strict browser-only deployments, set allowlist and `META_WS_ALLOW_NO_ORIGIN=false` (note: native FFI clients may need a different deployment or policy).

