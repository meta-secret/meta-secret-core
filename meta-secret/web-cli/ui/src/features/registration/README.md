# Orbit registration

Production entry: `components/registration/OrbitRegistration.vue`, mounted by
`views/VaultView.vue`. Global `PasskeyAuth.vue` is unchanged: existing browser
key bootstrap/authentication still precedes registration. Successful canonical
Member plus completed Core sync opens the existing Vault UI.

- `registrationController.ts`: typed presentation and user intents, one active
  submission, generation guard, persisted Core Pending/Declined mapping.
- `wasmRegistrationAdapter.ts`: existing WASM operations, serialized access,
  authenticated request-scoped JOIN stream, sync-before-canonical-read.
- `emailProvider.ts`: optional real provider email selection. This email is a
  public Vault name suggestion, never authorization to join or read secrets.
- `cameraScanner.ts`: real qr-scanner lifecycle. QR selects an exact plain Vault
  name (including CLI names), then submits/resumes the normal Core approval
  request. Unknown names do not create a Vault. No serverless/invite redemption
  contract is introduced.
- `qrAvailability.ts`: Web QR is available only on phones/tablets with an
  accessible video input in a secure context. Computers stay disabled even
  with a camera or touchscreen. Browser form-factor hints are preferred, with
  user-agent/iPadOS desktop-mode fallbacks; this is a best-effort presentation
  rule, not an authorization boundary. Missing or failed camera inventory
  leaves QR disabled. Inventory never calls `getUserMedia` or asks permission;
  the expanded sheet owns permission and camera lifetime. Capability refreshes
  on camera changes/return to foreground; stale results and disposed listeners
  cannot re-enable the control.

## Display a Vault QR on a connected device

Legacy `components/vault/Devices.vue` → **Add device** opens the existing
dialog with `VaultNameQr.vue`. Its payload is only the exact current
`AppState.getVaultName()` from Core: email, Apple relay or CLI name, with case
preserved. It contains no URL, JSON, credentials or key material. The full name
is visible beside the code. Scanning selects that name and sends the ordinary
JOIN request; a connected member must still approve it through Core.

Displaying this QR works on desktop as well as mobile and requests no camera
access. The phone/tablet restriction above applies only to scanning. No name
means disabled generation. Changing/disappearing canonical name closes the
dialog; closing/unmounting clears SVG and invalidates late rendering. Existing
qr-code-styling renders square black modules on white with a quiet zone. Its
1.9.2 legacy Byte boundary receives explicit UTF-8 bytes so Unicode CLI names
round-trip without changing the canonical name. An
encoder failure shows a short error, with retry by closing/reopening.

Focused component tests cover exact payload, source, missing/changed name,
close/unmount, stale results and encoder errors. An additional real-encoder
matrix check covers UTF-8 Cyrillic, CJK/emoji and ASCII. The captured Web QR PNGs
for ASCII and Cyrillic also decode to the exact sample name with macOS Vision.
These are software checks; physical-camera scan and Core JOIN E2E remain
separate acceptance checks.

`vault-qr-preview.html` is a DEV-only visual entry for the exact props-only
`VaultNameQrDialog.vue`, with an editable public sample name and a visible
simulated-Core notice. It calls no AppState/authentication/network operation and
is excluded from the default production build. This is a component preview,
not registration or Core E2E evidence.

Back from canonical Pending or Declined clears the unfinished local registration
and returns01 with an empty name. The adapter captures the initialized manager,
canonical Vault name and exact public Core device identity, syncs and rereads
state, then requires the same manager/name/device and Pending or Declined
immediately before cleanup. Same-attempt Pending→Declined remains eligible.
Missing/unreadable state or identity/context changes fail without deletion; a
discovered Member is preserved. Cleanup completion requires an actual initialized
Local state with a new nonempty public device ID different from the old attempt.
Failure remains an actionable error on04, not a false01. Duplicate Back is
serialized and late callbacks are invalidated. Global WebAuthn/passkey
credentials are outside this registration cleanup.

The current Core API has no atomic reset-versus-remote-approval operation; do
not claim that an old server request was revoked. All other Back destinations,
provider selection, QR, motion and legacy Vault behavior remain unchanged.

## Provider deployment configuration

The browser repository previously had no Apple/Google integration. Both are
unavailable until public provider configuration is supplied:

| Vite build variable | Meaning |
| --- | --- |
| `VITE_GOOGLE_WEB_CLIENT_ID` | Google OAuth browser client with this app's authorized JavaScript origin |
| `VITE_APPLE_EMAIL_BROKER_URL` | Confirmed HTTPS origin of the configured MetaSecret Apple email broker |

These are public identifiers/URLs. Never put an OAuth client secret in a Vite
variable. Configure allowed origins/callbacks with the provider before testing
real accounts. Google uses GIS token popup and official userinfo endpoint;
Apple opens an external provider popup through the proof-bound email broker.
Only an opaque request ID returns to the browser; exact signed email is taken
once over HTTPS using an in-memory verifier. The broker verifies Apple's JWS,
state/nonce, audience, issuer, time and hybrid code hash; unsigned user.email is
never trusted. Apple tokens/code stay transiently at the broker and are discarded.
See [provider contract](../../../../../../docs/apple-email-provider.md).
No manual-input shortcut simulates provider success. An unavailable provider
is visibly disabled with a truthful configuration status.

Shared design assets are generated from root design-system0.2.0 and pinned by
Core repository `design-system.lock.json`. Do not edit generated exports.

## Balanced Web layout and availability

Expanded Web windows (at least 900×600 CSS pixels) use the approved Balanced
composition in a centered 900×500 frame. The frame scales uniformly by
`0.9 × min(1, (width − 110) / 900, (height − 190) / 500)`; smaller and short-height
Web windows retain the existing compact layout. Native layout and shared design
0.2.0 tokens/icons/fonts are unchanged. Web-only geometry lives in local scoped
styles rather than generated shared exports.

The base wordmark is 68.4px; the name and input share Manrope 42.3px, weight 400,
line-height 1.2 and tracking −.04em. Base circles are 77.4px and glyphs 30.6px,
before the whole-frame scale. The field keeps usable width bounds, follows the
rendered name/draft width, and has an 84.6px height with 13.5px radius. Long names
fit within the bounded heading area with the same fitted field typography;
placeholder `#7c8480` inherits that typography.

Coalesced presentation measurements use the real visible subtitle/name ranges
and matching canvas font metrics, after local fonts, resize and state/draft
changes. They align the field-border-to-actions gap with the visible status-text
gap, without a hidden status or new registration state. Measured reference gaps
at 1920×1080 and 1024×768 are about 74.925px (subpixel rounding below .02px).
Queued work/font/resize listeners are cleaned up on unmount and ignore late work.

Balanced uses symmetric fixed ruler ticks and badge center (82,210). Its accepted
desktop scene pivot remains (895,210), 760ms and ±22° with fade: forward clockwise,
Back counterclockwise. This is intentionally the prototype's pivot, separate
from the ruler-circle center. Compact Web retains its existing 40° token-based
motion. Reduced motion, inert/focus handling, loaders and request fusion remain;
desktop fusion spacing follows the larger circles. The QR sheet stays outside
the scaled frame, retaining its own compact controls and camera lifecycle.

On step02, a Core-derived `available` name enables Create and disables Join;
`exists` disables Create and enables Join. Disabled Join has no hover/icon motion
and the controller independently rejects forced activation unless step02 is
`exists`, not busy and not disposed. Checking/error states expose no active Join.
This is presentation intent gating of Core state, not a membership/authorization
decision: entering04 does not submit, and Request still invokes the ordinary
approval path. QR admission, providers, Passkey order, cleanup and legacy Vault
behavior are unchanged.

Component/controller regressions cover availability and stale/busy/error guards.
Actual local preview renders verify expanded/compact bounds, font/field gaps,
fixed ticks with both motion directions, disabled hover, long/Unicode names and
tablet QR geometry using simulated Core/camera ports. Those renders are not real
Core UI E2E. Existing manual Test51 cases add available/exists Join assertions via
their owning E2E author. See the phase-one disposition below for the unfinished
combined local Core gate; component checks do not certify it.

## Local visual preview

`orbit-preview.html` is an independent **development-only** Vite entry, guarded
by `import.meta.env.DEV`, excluded from the default production build. It mounts
the exact components with explicitly simulated Core/camera ports for visual
review; it never bypasses production PasskeyAuth or Core membership. It is not
an E2E substitute. A real browser camera requires a secure context (HTTPS or
localhost). Camera starts after sheet expansion and is destroyed on close,
background and unmount.

## Phase-one closeout

The owner accepted the current registration phase on 19 October 2026 and
authorized local commits without push. This release disposition does not turn
unverified live Apple/Google authorization, physical-camera behavior or the
incomplete final Core business UI E2E into passing results. Apple Web/Android
activation is deferred to the cofounder with Cloudflare access; optional Apple
configuration remains disabled. No new login, public tunnel or deployment is
part of this closeout.

The unreachable legacy `components/vault/auth/Registration.vue` was removed
together with its private `registrationVaultDraft.ts`, the helper's two obsolete
tests and `components/ui/progress/` (its only consumer was the old form).
`VaultView.vue` mounts Orbit. Shared legacy views, shadcn components, global
styles/fonts, public branding URLs and active QR/provider modules remain.

Both documented DEV preview entry groups are retained as useful isolated
component visual fixtures. They are guarded by `import.meta.env.DEV`, use
simulated ports and are excluded from the default production entry. Current
workflow receipts record post-cleanup checks, reused byte-identical evidence
and the exact local staging/commit disposition; generated captures are not
application source or account acceptance.
