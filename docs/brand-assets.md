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
| `docs/img/meta-secret-logo.png` | Current README and technical presentation mark | Same 400px PNG |

The exact manifest and maintenance instructions live at workspace-root
`design-system/brand/manifest.json` and `design-system/brand/README.md`. Core
builds and runtime do not depend on the sibling package or renderer. Files are
copied only after shared integrity checks and independent mask/visual approval.
The existing Orbit design package, client source, styles and lock stay unchanged.

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
