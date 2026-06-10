## Context

The current Markdown preview shell is implemented as a `GtkPaned` inside the
main window's `"tabs"` stack page. The editor tab view is the start child and
`LushtextMarkdownPreview` is the end child. `window/preview.rs` owns three
states on top of that paned widget: editor-only, side-by-side preview, and
preview-only mode.

That implementation carries app-owned geometry behavior that Libadwaita already
provides for utility panes: explicit `AdwTimedAnimation`, paned position
callbacks, temporary `shrink-start-child`/`shrink-end-child` changes, a
`preview-animation` readiness blocker, debounced position persistence, and
rules that exist mainly to keep paned transitions warning-free. GTK Lush's
program constitution says Adwaita remains authoritative for adaptive behavior,
so this shell must be simplified before reusable GTK Lush geometry or settle
APIs are extracted.

The user contract is broader than the widget choice. The change must keep
preview-only mode on `Alt+P`, keep side-by-side preview available for
diagnostic and scenario setup actions, preserve Focus Mode interactions, keep
automation state fields stable, and continue repairing embedded Markdown code
block widths after the preview surface becomes visible.

## Goals / Non-Goals

**Goals:**

- Replace the preview `GtkPaned` presentation with Adwaita-native layout
  containers and delete the preview-owned paned animation path.
- Preserve the three existing visible preview states: editor-only,
  side-by-side preview, and preview-only mode.
- Keep existing GTK/GIO action names, action state meanings, automation
  snapshot fields, and documented preview shortcuts unless a spec delta and
  documentation update explicitly changes them.
- Treat `preview-pane-position` as legacy preferred side-by-side preview width,
  or migrate it deliberately, so existing users keep a reasonable preview size.
- Update rules, docs, widget tests, template contracts, and visual proof so the
  repo no longer teaches preview-specific paned workarounds.

**Non-Goals:**

- Do not extract a GTK Lush crate in this change.
- Do not redesign Markdown rendering, parsing, syntax highlighting, or code
  block styling.
- Do not add a new user-facing preview command or make side-by-side preview
  auto-restore at startup.
- Do not replace the existing workspace or document-properties adaptive shells.
- Do not introduce a custom layout engine, view DSL, or app-owned animation
  scheduler around Libadwaita containers.

## Decisions

### Use a nested preview layout view with an end `AdwOverlaySplitView`

The `"tabs"` stack page will be refactored around a small preview presentation
layout. The preferred structure is a nested `AdwMultiLayoutView` that owns two
slots:

- editor slot: the existing `editor_box`/`AdwTabView`
- preview slot: the existing `LushtextMarkdownPreview`

Its normal editing layout uses an `AdwOverlaySplitView` with the editor slot as
content and the preview slot as the end-position sidebar. Editor-only mode is
the same layout with `show-sidebar=false`; side-by-side mode is the same layout
with `show-sidebar=true`.

Its preview-only layout places the preview slot as the full content area. This
keeps a single preview widget and avoids manual reparenting or duplicate
renderers while matching the current full-width preview-only behavior.

Alternatives considered:

- Plain `AdwOverlaySplitView` only: good for editor-plus-preview utility pane,
  but it does not by itself provide a clean full-content preview-only state for
  the same preview widget.
- Extend the existing document-properties `AdwMultiLayoutView` with preview as
  another global slot: possible, but it couples preview, document properties,
  and compact secondary-surface arbitration more than this prerequisite change
  needs.
- Keep `GtkPaned` and only delete custom animation: leaves the source surface
  GTK Lush is trying to remove and keeps paned-specific rules alive.

### Preserve preview actions and state semantics

`win.toggle-preview-pane` and `win.set-preview-pane-visible` continue to mean
"request side-by-side preview." `win.toggle-preview-mode` and
`win.set-preview-mode` continue to mean "render Markdown as the focused content
area." The two presentation modes remain mutually exclusive: entering
preview-only hides side-by-side preview, and opening side-by-side preview exits
preview-only.

Side-by-side preview remains reset to hidden on application startup. Existing
tests and automation rely on target-state actions to enter the state they need;
this change should keep that convergence contract instead of adding implicit
startup restoration.

### Keep compact behavior explicit and Adwaita-owned

When the window is compact enough for `AdwOverlaySplitView` to collapse, the
side-by-side preview is treated as an explicitly requested preview secondary
surface, not as passive resize fallout. The implementation may use
Libadwaita's collapsed/overlay presentation for that explicit state, but it
must not persist an overlay-obscured editor state caused only by passive
resizing.

Preview-only mode remains the primary compact-friendly user command because it
fills the content area without trying to divide narrow width between editor and
preview.

### Convert persisted paned position to preferred preview width

The existing `preview-pane-position` key is already effectively used as a
stored preview width. The implementation should keep the key as a legacy
preferred side-by-side preview width during this change unless a separate
settings migration proves a cleaner rename is worth the churn. The schema
description and code comments should stop describing it as a paned divider
position once the paned widget is gone.

The preferred width must be clamped against the current available content width
so the editor keeps a usable viewport and the preview does not exceed the
existing "about one third of the content width" identity.

### Readiness tracks preview presentation work, not paned animation

The implementation should delete `preview_animation` and
`preview_animation_active` as paned-specific state. If preview presentation work
still has asynchronous settle points, readiness must track those points through
a shell-neutral name or a compatibility mapping. The least disruptive path is
to keep documented automation behavior stable unless the change updates
`docs/automation.md`, `docs/automation-reference.md`, the action catalog, and
client self-tests in the same patch.

### Update tests and rules to prove the new contract

Animation-specific tests that assert a nontrivial `preview_animation` should be
removed or rewritten as Adwaita shell-settle tests. Behavior tests should stay:
target-state actions converge, side-by-side and preview-only remain mutually
exclusive, Focus Mode suppresses/restores side-by-side preview, code blocks
recompute width after preview visibility changes, large documents show the
preview fallback, and automation snapshots report preview state accurately.

Rules should stop teaching preview paned shrink/resize choreography and instead
document the new Adwaita-native preview shell and the remaining warning-free
proof responsibilities.

## Risks / Trade-offs

- Preview-only could accidentally become a narrow overlay instead of full
  content -> Use `AdwMultiLayoutView` slots so preview-only has its own layout
  and widget tests assert the editor is not visible in that mode.
- Compact side-by-side preview could obscure editor content after passive
  resize -> Treat collapsed preview visibility as explicit requested state and
  add compact-width coverage for passive resize versus target-state activation.
- Replacing `GtkPaned` can break `TemplateChild` bindings and generated template
  contracts -> Update Rust `TemplateChild` fields and regenerate both
  `window.ui` and `template-contract.json`; run template drift validation.
- Code-block widgets may keep stale narrow widths when the preview moves
  between layouts -> Trigger the existing embedded code-block layout refresh
  after the Adwaita layout has allocated the preview slot.
- Automation consumers could time out waiting for the old blocker name -> Keep
  the external readiness vocabulary stable where practical, or update all
  automation documentation and self-tests in the same change.
- Libadwaita collapsed behavior may differ subtly from the old paned animation
  -> Prefer behavior-level widget and visual assertions over matching old pixel
  paths, and require warning-free captures at normal, compact, and short
  geometries.
