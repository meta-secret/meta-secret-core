# Gray01 brand assets

Core Web consumes static exports from the shared MetaSecret brand pack 1.0.0.
The owner approved the original five-path mark with fills `#2C2E2F`, `#555B5D`,
`#858E91` and the unchanged white keyhole on 18 October 2026.

| Core resource | Use | Shared export |
|---|---|---|
| `meta-secret/web-cli/ui/public/logo.png` | Navbar at 28px and 24px; 400×400 gray mark/glow canvas | `exports/web/logo.png` |
| `meta-secret/web-cli/ui/public/logo.jpeg` | Served legacy filename; 500×500 canonical mark on black | `exports/web/logo.jpeg` |
| `meta-secret/web-cli/ui/public/favicon.ico` | Current HTML favicon; 256×256 PNG-backed ICO | `exports/web/favicon.ico` |
| `meta-secret/web-cli/ui/public/favicon1.ico` | Served legacy alias | Same ICO |
| `meta-secret/web-cli/ui/public/favicon2.ico` | Served legacy alias | Same ICO |
| `meta-secret/web-cli/ui/public/metasecret-gray.svg` | Passkey setup/unlock dialog; 80px black backplate | Exact canonical `metasecret-gray.svg` |
| `docs/img/meta-secret-logo.png` | Current README and technical presentation mark | Same 400px PNG |

The exact manifest and maintenance instructions live at workspace-root
`design-system/brand/manifest.json` and `design-system/brand/README.md`. Core
builds and runtime do not depend on the sibling package or renderer. Files are
copied only after shared integrity checks and independent mask/visual approval.
The existing Orbit design package, styles and lock stay unchanged.

The Passkey dialog uses a finite local copy of the canonical five-path SVG:
SHA-256 `ee3bcc2793983da575f7723703115b9c89c18267ed1c97f006226c84b354d69b`.
Copy the source SVG directly; do not redraw, tint or modify the shared brand
package/manifest. Web builds copy this public SVG unchanged into `dist`.

Its wording distinguishes “Set up MetaSecret” for a new browser credential from
“Unlock MetaSecret” for an existing one. It describes a passkey without promising
an existing Vault or biometric hardware as the only method. The prompt remains
in its existing position: authentication obtains the local Master Key before
Core initialization and registration. Before that initialization, unknown
membership does not prove that no Vault exists. Moving the prompt behind an
Orbit-first entry remains an open product/security decision; this visual update
does not change auth/store/router/Core/storage ordering or any button action.

The 400px PNG keeps its original pixel geometry, glow support and opaque alpha;
only brand tone changes. Public JPEG/ICO aliases deliberately replace older
identity resources. Historical documentation JPG/Grok images, presentation PDF,
screenshots and unrelated `approve_request.png` remain unchanged.

Verify copied/built files against the shared manifest SHA-256 and dimensions.
Run ordinary Web lint, typecheck, build and unit checks with existing WASM
artifacts; no Rust/native rebuild is needed for image copies. Review a gallery
of the actual built resources, including 24px/28px use. This does not claim live
Core/account E2E. A browser may need a normal reload or favicon cache refresh to
display changed static resources; do not clear application storage or user data.
