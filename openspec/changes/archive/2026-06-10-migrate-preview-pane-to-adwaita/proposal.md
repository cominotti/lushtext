## Why

The GTK Lush program cannot freeze reusable geometry or settle APIs while
LushText still carries a hand-animated `GtkPaned` preview pane that duplicates
Libadwaita behavior and requires special paned-animation rules. This reserved
follow-up satisfies the `gtk-lush-program-governance` roadmap by shrinking the
LushText-only preview shell before any GTK Lush extraction phase depends on it.

## What Changes

- Replace the Markdown side-by-side preview pane's `GtkPaned` presentation with
  an Adwaita-native container, with `AdwOverlaySplitView` as the preferred
  direct replacement for the editor/preview utility-pane relationship.
- Remove preview-owned paned position animation, `shrink-*` choreography, and
  related readiness/persistence plumbing that exists only to make `GtkPaned`
  behave like an adaptive utility pane.
- Preserve the existing user contract for preview-only mode, side-by-side
  preview, target-state actions, focus-mode preview behavior, large-document
  fallback, code-block layout repair, and automation snapshots.
- Keep preview layout state compatible with existing settings by treating the
  old pane-position value as a legacy preferred preview width or migrating it
  deliberately without losing user intent.
- Retire or rewrite paned-specific project rules and tests in favor of the
  Adwaita-native preview presentation and warning-free visual/layout proof.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `adaptive-editor-geometry`: Define the Markdown preview pane as an
  Adwaita-native secondary surface whose layout, compact behavior, readiness,
  and visual geometry settle without app-owned `GtkPaned` animation.
- `dbus-automation-spine`: Preserve the existing preview action, snapshot, and
  readiness semantics while the implementation moves from paned animation to
  Adwaita-managed presentation.
- `desktop-visual-smoke-coverage`: Require real-session proof for the
  side-by-side preview shell as well as preview-only mode when this
  geometry-sensitive preview migration is implemented.
- `focus-writing-mode`: Preserve side-by-side preview suppression/restoration
  and preview-only mode inside Focus Mode with the new preview shell.
- `markdown-preview-code-blocks`: Reframe code-block width repair around
  preview visibility and allocation changes rather than `GtkPaned` divider
  position.
- `ui-template-source-fidelity`: Authorize the intentional main-window template
  contract change from the preview `GtkPaned` node to an Adwaita-native preview
  utility-pane node while preserving unrelated template fidelity.

## Impact

- Affected UI shell code: `resources/ui/window.blp`,
  `resources/ui/window.ui`, `resources/ui/template-contract.json`,
  `crates/lushtext-core/src/ui/window/preview.rs`, window template children,
  focus-mode preview integration, automation readiness/state collection, and
  preview-related widget tests.
- Affected settings/docs/rules: preview pane position/visibility settings,
  `docs/automation.md`, `docs/automation-reference.md` if exposed semantics
  change, `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and
  `.agents/rules/build.md` paned-warning guidance.
- Verification impact: the phase boundary must keep the full LushText gate set
  green, including widget coverage, automation documentation checks when
  automation-visible contracts move, Blueprint/template drift checks, warning
  scans, and visual-geometry proof for the geometry-sensitive template change.
