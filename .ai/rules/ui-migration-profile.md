# UI migration applicability profile

Use this profile with the existing repository WORKFLOW.md stages for the approved registration and Liveboard migrations. It changes applicability and evidence, not stage ownership or Core contracts. Record `Profile: ui-migration` and each stage's Required/Not applicable decision in the plan. This scoped profile takes precedence over Rust-only commands, mandatory percentage wording and Figma-only review conditions in older agent examples.

Liveboard run20261019210543 is authorized20October2026: validated root shared
design exports first, then Web, then shared Compose Android/iOS. Root brief:
`UxRevolution/implementation/liveboard-task.md`. All settled UI/motion states
must match; business/protocol/persistence changes require the owner's applicable
agreement and Core-first contract handoff. Existing stages/gates remain intact.

## Scope and order

- Workspace-root `design-system/` owns versioned tokens, source SVG/assets and deterministic CSS/TypeScript/Kotlin exports. `UxRevolution/` owns approved research, prototypes, motion specifications, screenshots and migration records; it is not a runtime dependency.
- Export and validate the shared design package first. Web (in Core repository) and shared Compose UI can then integrate the same pinned version independently. Editing Vue does not imply a Rust or FFI change.
- Protocol/domain/FFI changes still require Core-first validation and an explicit contract handoff before Compose. Do not modify Core behavior merely to reproduce a prototype shortcut.
- Keep unrelated screens on their existing design system. Track replaced files and deletion conditions in `LEGACY-MIGRATION.md`, rather than adding removal markers throughout source.

## Existing stages and relevant evidence

| Stage responsibility | UI migration evidence |
|---|---|
| Intake / clarification | Approved task brief, reference commit/files and settled session decisions; ask only unresolved consequential questions |
| Planning / constraint validation | State-to-Core mapping, files/layers, package version, phone/tablet/window/orientation matrix, permission and error paths |
| Test author / implement / refactor | Meaningful controller/ViewModel transition tests; visual/motion capture for appearance; do not invent behavioral unit tests for a reversible color or spacing change |
| Build | Changed client typecheck/build and mobile platform compilation; reuse unchanged native/WASM artifacts with recorded identity |
| Security / code review | Core authority, port boundaries, provider/camera permissions, cleanup/request identity, no secret logging; Rust clippy/crypto audit only if corresponding code changes |
| Design review | Required for any changed UI, even without architecture changes or Figma; compare approved HTML/CSS/JS prototype plus screenshots and motion specification |
| Coverage | Run applicable configured reporting/validation and record measured scope; overall 80% is advisory, never the sole failure reason. Missing tooling must be reported, not disguised as measured coverage; required behavior tests still pass |
| Tests / final UI E2E | Cheap component/transition checks first, then focused local real-Core scenarios through the existing final-gate skill. State selected scenarios, roles and repetitions before expensive execution; do not silently invoke the entire historical suite |
| Approval / release | Record explicit existing session authorization per action. Commit permission does not imply push, PR, merge or deploy permission; ask only for a still unauthorized action after presenting a concrete result |

Always create the existing stage artifact. A non-applicable Rust build/coverage/diagram command is recorded with a reason and the client check that replaces it; it is not a reason to skip review. Documentation-only preparation uses dependency/link/diff checks, not application builds. Runtime stage failures remain blocking; reduce scope or report the concrete blocker instead of weakening assertions.

## Visual contract

Compare every changed state and transition, including loading, disabled, error, decline, pending, resume, permissions and navigation. Capture relevant compact/expanded widths and portrait/landscape layouts, keyboard/focus, pointer hover and reduced motion. Use actual supported platforms, not browser-only assumptions about native effects. No unapproved visual approximation: surface an unsupported effect or missing asset before declaring fidelity. Native OS permission/authentication dialogs retain system rendering.

## Layer boundary

Views render typed presentation state and emit intents. Local focus, hover and animation progress are presentation details. Controllers/ViewModels coordinate operations via injected client services/Core ports; platform adapters own OS integration. Core alone decides membership, authorization, request results and synchronization readiness. QR recognition never grants membership by itself. Preserve the existing Core approval flow unless a separately approved contract explicitly permits another path.
