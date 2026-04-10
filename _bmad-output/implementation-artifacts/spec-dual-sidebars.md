---
title: 'Dual sidebars with overlay split views'
type: 'feature'
created: '2026-04-10'
status: 'done'
baseline_commit: '5102a111cd56dc2c8bd31bd2ff5406e4ad111f0b'
context:
  - 'docs/next/dual-sidebars.md'
  - '.agents/rules/ui.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** LushText still drives the left workspace pane through a custom `GtkPaned` animation path that has needed repeated geometry fixes and does not extend cleanly to a second utility pane. That makes the current shell a poor base for the dual-sidebar layout proposed in `docs/next/dual-sidebars.md`.

**Approach:** Replace the outer shell with nested `AdwOverlaySplitView`s so the workspace and properties panes use the same toolkit-owned toggle model on desktop and collapsed widths. Keep the current editor/search/preview stack as the center host, migrate legacy left-sidebar state into split-view settings, and ship a first useful right pane with current-document metadata plus the existing GSettings-backed editor controls for word wrap, line numbers, current-line highlight, tab width, and insert-spaces.

## Boundaries & Constraints

**Always:** Use nested `AdwOverlaySplitView`s for the left workspace and right properties panes. Keep `win.toggle-sidebar` in the status bar, add header-bar action `win.toggle-properties`, preserve the current search panel and markdown preview inside the central content host, collapse the right pane before the left pane at breakpoints, migrate `sidebar-visible` and `sidebar-position` once into new split-view settings, clamp stored fractions, and update docs plus widget coverage with the code.

**Ask First:** If libadwaita 1.8 forces a different split-view type, if keeping the status-bar toggle becomes incompatible with the new shell, if a new keyboard shortcut is needed for the properties pane, or if the right pane must grow beyond file path, encoding, file size, EditorConfig state, and the listed formatting controls in this slice.

**Never:** Keep `GtkPaned` as the outer shell, preserve arbitrary drag-resizable widths as a compatibility goal, reintroduce custom left-sidebar animation choreography, or ship an empty right pane. Do not regress search panel, preview pane, or focus-restoration behavior.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Migrated desktop restore | Existing install has `sidebar-visible` and pixel `sidebar-position` saved | First post-migration window restores workspace visibility and derives a clamped width fraction; properties pane starts hidden | Invalid or out-of-range stored values clamp to the supported range and the window still opens |
| Wide-window dual panes | Window is wider than both breakpoints and an editor is active | Workspace and properties panes toggle independently and may stay visible together beside the center host | N/A |
| Collapsed overlay or missing metadata | Window drops below breakpoints, or the properties pane opens with no file-backed editor | Toggles switch to overlay behavior cleanly, focus returns predictably on dismiss, and unavailable metadata rows render as empty/unavailable state | Missing file-backed values never panic or show stale data |

</frozen-after-approval>

## Code Map

- `resources/ui/window.ui` -- Replace the outer shell with nested `AdwOverlaySplitView`s and add the properties toggle affordance
- `crates/lushtext-core/src/ui/window/imp.rs` -- Swap paned/revealer/animation template state for split-view, breakpoint, and migration state
- `crates/lushtext-core/src/ui/window/mod.rs` -- Rebind actions, restore/persist split-view settings, remove the custom left-sidebar animation path, and preserve focus plus status/header refresh behavior
- `crates/lushtext-core/src/config.rs` and `data/dev.cominotti.lushtext.gschema.xml` -- Define the new workspace/properties visibility and width-fraction keys and retain legacy left-sidebar keys for migration
- `crates/lushtext-core/src/ui/properties_panel/mod.rs`, `crates/lushtext-core/src/ui/properties_panel/imp.rs`, and `resources/ui/properties-panel.ui` -- Add the new right-side properties panel with file metadata and existing formatting controls
- `crates/lushtext/tests/widget/window.rs` -- Replace paned-specific sidebar regressions with split-view, breakpoint, migration, and focus coverage
- `README.md`, `AGENTS.md`, and `.agents/rules/ui.md` -- Refresh the documented window hierarchy and sidebar behavior

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/window.ui` -- Replace `main_paned` with nested `AdwOverlaySplitView`s and keep the current editor/search/preview subtree as the inner content host so the shell changes without scattering central-content ownership
- [x] `crates/lushtext-core/src/ui/window/imp.rs` and `crates/lushtext-core/src/ui/window/mod.rs` -- Migrate template bindings, add split-view actions and breakpoint handling, persist new visibility/fraction state, and delete paned-only sidebar code that no longer applies
- [x] `crates/lushtext-core/src/config.rs` and `data/dev.cominotti.lushtext.gschema.xml` -- Add workspace/properties split-view keys and perform one-shot migration from legacy `sidebar-visible`/`sidebar-position`
- [x] `crates/lushtext-core/src/ui/properties_panel/mod.rs`, `crates/lushtext-core/src/ui/properties_panel/imp.rs`, and `resources/ui/properties-panel.ui` -- Add the right-side panel with a safe empty state plus file path, encoding, file size, EditorConfig state, and the existing formatting controls when an editor is active
- [x] `crates/lushtext/tests/widget/window.rs` -- Replace paned-motion assertions with split-view behavior tests that cover independent toggles, right-before-left collapse order, migrated settings restore, and focus after overlay dismissal
- [x] `README.md`, `AGENTS.md`, and `.agents/rules/ui.md` -- Update the documented window hierarchy and sidebar behavior so the repo docs match the new shell

**Acceptance Criteria:**
- Given a user upgrades from the current build with `sidebar-visible` and `sidebar-position` saved, when the first post-migration window opens, then the workspace sidebar visibility carries forward and the remembered left width is converted into a clamped split-view fraction saved under the new keys
- Given a wide window with an active editor, when `win.toggle-sidebar` and `win.toggle-properties` are activated, then the workspace and properties panes can each be shown or hidden independently and both can remain visible beside the central stack
- Given the window crosses the configured breakpoints, when the properties pane collapses before the workspace pane, then the same toggle actions now control overlay visibility and closing an overlay restores focus predictably
- Given the properties pane is opened while no file-backed editor is active, when it renders its contents, then file-backed metadata rows are shown as empty or unavailable state instead of assuming a path-backed document exists
- Given the feature is exercised in a live app run, when both panes are toggled repeatedly across resize boundaries, then there are no GTK measurement warnings and the search panel plus markdown preview still behave correctly inside the central content host

## Spec Change Log

## Verification

**Commands:**
- `cargo test -p lushtext --test widget window -- --nocapture` -- expected: window widget coverage passes with split-view behavior replacing paned-only assumptions
- `cargo test -p lushtext --test widget` -- expected: full widget suite remains green with the new shell in place
- `cargo fmt --check` -- expected: formatting is clean
- `cargo clippy --all-targets -- -D warnings` -- expected: no new warnings

**Manual checks (if no CLI):**
- `make run` -- expected: restored workspaces load, both pane toggles behave correctly across breakpoint changes, focus returns cleanly after overlay dismissal, and stderr stays free of GTK measurement warnings

## Suggested Review Order

**Shell Layout**

- Replaces the outer paned shell with nested split views while preserving the editor/search host.
  [`window.ui:70`](../../resources/ui/window.ui#L70)

- Restores defaults, migration, breakpoints, and pinned visibility from one shell entry point.
  [`imp.rs:241`](../../crates/lushtext-core/src/ui/window/imp.rs#L241)

- Rebinds workspace and properties toggles to split-view state and post-close focus handling.
  [`mod.rs:768`](../../crates/lushtext-core/src/ui/window/mod.rs#L768)

**Properties Panel**

- Adds the new right-side panel structure with document metadata and reused formatting controls.
  [`properties-panel.ui:6`](../../resources/ui/properties-panel.ui#L6)

- Computes file-backed versus unavailable metadata states without introducing a new service layer.
  [`mod.rs:30`](../../crates/lushtext-core/src/ui/properties_panel/mod.rs#L30)

**Settings and Migration**

- Declares the new split-view persistence keys alongside the legacy migration inputs.
  [`gschema.xml:72`](../../data/dev.cominotti.lushtext.gschema.xml#L72)

- Exposes the same keys to Rust callers through the central config module.
  [`config.rs:26`](../../crates/lushtext-core/src/config.rs#L26)

**Verification and Docs**

- Replaces the old paned-focused window suite with split-view, breakpoint, migration, and focus regressions.
  [`window.rs:177`](../../crates/lushtext/tests/widget/window.rs#L177)

- Updates the public architecture overview to match the dual-sidebar shell.
  [`README.md:3`](../../README.md#L3)

- Syncs the contributor-facing architecture notes and UI rules with the new layout.
  [`AGENTS.md:65`](../../AGENTS.md#L65)

- Documents the canonical widget hierarchy and persistence rules for future UI work.
  [`ui.md:16`](../../.agents/rules/ui.md#L16)
