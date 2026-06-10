## 1. Preview Shell Template

- [x] 1.1 Replace `GtkPaned#preview_paned` in `resources/ui/window.blp` with a nested preview presentation using `AdwMultiLayoutView` slots for the editor and Markdown preview.
- [x] 1.2 Add the normal editing layout with an end-position `AdwOverlaySplitView` so editor-only and side-by-side preview are driven by `show-sidebar`.
- [x] 1.3 Add the preview-only layout that places the existing `LushtextMarkdownPreview` as the full content area without duplicating or manually reparenting the widget.
- [x] 1.4 Update `LushtextWindow` template children and initialization code to bind the new layout/split-view nodes and remove the `preview_paned` child.
- [x] 1.5 Regenerate `resources/ui/window.ui` and `resources/ui/template-contract.json` with `make blueprint-generate`.

## 2. Preview Workflow State

- [x] 2.1 Rework `crates/lushtext-core/src/ui/window/preview.rs` so `win.toggle-preview-pane`, `win.set-preview-pane-visible`, `win.toggle-preview-mode`, and `win.set-preview-mode` drive the Adwaita preview layout while preserving action names, state values, active-tab enablement, and mutual exclusion.
- [x] 2.2 Delete preview-owned `GtkPaned` position animation, `AdwTimedAnimation`, `shrink-start-child`/`shrink-end-child` choreography, and paned-specific callbacks/state from `window/preview.rs` and `window/imp.rs`.
- [x] 2.3 Convert `preview-pane-position` usage into a legacy preferred side-by-side preview width, clamp it against the current content width, and update schema text/comments so it no longer claims to be a paned divider position.
- [x] 2.4 Keep side-by-side preview hidden on startup while preserving explicit target-state setup through actions and automation.
- [x] 2.5 Preserve preview rendering behavior for Markdown documents, non-Markdown placeholders, large-document pause messages, active-buffer refreshes, and existing render debounce semantics without folding in the later debounce-normalization phase.
- [x] 2.6 Trigger embedded Markdown code-block layout refresh after preview visibility, layout-view switches, split-view show/hide, preferred-width changes, and Focus Mode readable-column changes settle.

## 3. Integration Contracts

- [x] 3.1 Update Focus Mode integration so side-by-side preview is suppressed on entry, restored on exit only when appropriate, and never leaks a collapsed preview overlay over the focused writing surface.
- [x] 3.2 Verify new-document, close-tab, tab-switch, and search/replace workflows keep their existing preview-only exit or refresh behavior after the shell migration.
- [x] 3.3 Preserve Automation1 snapshot fields `surfaces.preview_pane_visible` and `surfaces.preview_mode` with shell-independent meanings.
- [x] 3.4 Replace or compatibility-map `preview-animation` readiness so `visual-geometry-settled` and `idle` wait for any preview layout transition or code-block repair before reporting ready.
- [x] 3.5 Update `docs/automation.md`, `docs/automation-reference.md`, the action catalog, and automation client self-tests if any exposed action, snapshot field, readiness blocker, predicate detail, or helper behavior changes.
- [x] 3.6 Extend visual smoke coverage so preview-only and side-by-side preview captures are distinct and verify preview state through automation before screenshot acceptance.

## 4. Tests And Proof

- [x] 4.1 Replace widget tests that assert a nontrivial `GtkPaned` preview animation with final-state, allocation, action-state, and warning-free Adwaita preview shell assertions.
- [x] 4.2 Add or update widget tests for editor-only, side-by-side preview, preview-only mode, compact side-by-side behavior, action convergence, and mutual exclusion.
- [x] 4.3 Add or update Focus Mode widget coverage for side-by-side suppression/restoration and focused preview-only mode under the new shell.
- [x] 4.4 Add or update Markdown preview code-block tests proving hidden-to-visible and side-by-side allocation changes recompute embedded code-block widths without false horizontal scrolling.
- [x] 4.5 Add or update automation/visual-smoke assertions for side-by-side preview state, preview-only state, bounded snapshots, and warning-scan artifacts.
- [x] 4.6 Run `make check-blueprint` after template regeneration and fix any stale template-contract or generated-output drift.

## 5. Docs, Rules, And Guidance

- [x] 5.1 Update `.agents/rules/ui.md` to describe the Adwaita-native preview presentation instead of the preview `GtkPaned` tree.
- [x] 5.2 Update `.agents/rules/widget-wiring.md` and `.agents/rules/build.md` to retire preview-specific paned-animation guidance while preserving any still-valid generic paned or live-warning rules.
- [x] 5.3 Update visual-smoke or end-user coverage documentation for the new side-by-side preview proof path.
- [x] 5.4 Update `docs/next/gtk-lush.md` only if implementation changes the Phase 0 ordering, reserved follow-up scope, or program principles.
- [x] 5.5 Run `make check-agent-docs` after rule or skill guidance changes and fix any rule index or documentation drift.

## 6. Phase Boundary Verification

- [x] 6.1 Run `openspec validate --changes --strict` and fix any delta-spec issues.
- [x] 6.2 Run `openspec validate --specs --strict` and fix any canonical-spec issues.
- [x] 6.3 Run `openspec validate --all --strict` and fix any full OpenSpec validation issues.
- [x] 6.4 Run `git diff --check` and fix whitespace or conflict-marker issues.
- [x] 6.5 Run `make check` and fix formatting, Clippy, policy, Blueprint, automation-doc, visual-proof-policy, and GTK Lush policy failures.
- [x] 6.6 Run `make test-widget-headless` and fix any widget harness regressions.
- [x] 6.7 Run `make visual-geometry-smoke` when visual-sensitive files changed and preserve the passing artifact summary expected by `make check-visual-proof-policy`.
- [x] 6.8 Run `make visual-smoke` to prove preview-only and side-by-side preview real-session captures remain nonblank, state-verified, and warning-free.

## Evidence Notes

- `resources/ui/window.blp`, generated `resources/ui/window.ui`, and `resources/ui/template-contract.json` were refreshed with `make blueprint-generate`; `make check-blueprint` and `make check` passed.
- Preview behavior was verified with the focused Mutter widget case, `make test-widget-headless`, `make visual-smoke`, and `make visual-geometry-smoke`.
- Automation and docs drift were checked with `make check-automation-docs`, `make check-agent-docs`, and `make check`.
- `.agents/rules/build.md` was reviewed; it has generic GtkPaned warning guidance but no preview-specific paned-animation contract to retire.
- `docs/next/gtk-lush.md` remained unchanged because this implementation did not alter Phase 0 ordering, reserved follow-up scope, or program principles.
