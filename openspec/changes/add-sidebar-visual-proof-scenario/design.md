## Context

- `cargo-gtk-proof` runs same-session before/after capture pairs. Each case activates one primary window action (`toggle-sidebar`, `toggle-command-palette`, `open-recent`, `preview-search-panel-replacements`), waits on readiness, and captures a screenshot plus an Automation1 snapshot. Assertions come from three sources:
  1. protected regions compared for exact pixel equality;
  2. relationship checks over the snapshot's `window.visual_geometry` surfaces (`runner.rs::evaluate_allowed_region_relationships`, `geometry.rs::surface_box`);
  3. screenshot-derived pixel anchors (`png.rs` detectors).
- Scenario types are a closed set in `model.rs` validation, `live.rs::primary_action_name`/fixture preparation, and `runner.rs` relationships. `replace-preview` is Rust-only: the Python oracle does not know it, and the same applies here.
- The visual proof policy exists twice: the authoritative `crates/cargo-gtk-proof/src/policy.rs` and the mirror `scripts/check-visual-proof-policy.py`. They share constants such as `SHELL_GEOMETRY_ROLE_HOME_PREFIX` and the invariant IDs.
- The sidebar already has the pieces a reveal needs:
  - `workspace_section/folder_execution.rs::select_and_scroll_to(path)` selects a row and calls `file_tree_view.scroll_to(i, FOCUS)`;
  - `pending_selection` is consumed after a folder-model restore;
  - expansion goes through admitted, generation-guarded child scans in `scan_admission.rs`/`scan_execution.rs`;
  - `workspace-tree-refresh` readiness covers the scans.
  What is missing is an entry point that resolves an absolute path to its section and ancestor chain and drives those steps in order.
- Headless constraints that rule out every existing route (recorded 2026-09-14): under headless Mutter `atspi-key` never reaches the toplevel; tree rows expose no AT-SPI actions; top-level folder menus lack Focus Folder; opening a file does not reveal its row.
- The rendered-bounds invariant from the slice-bin work (`workspace_tree_virtualization.rs`, the "Rendered-row stability" block) has three parts: a row is drawn where the bin placed it, relative to its section header; rows are drawn contiguously; and they stay still across forced layouts. Adjustment-value tests cannot see the class of defect this catches (v0.8.1, drawn 1–3px off).
- `WFR-WORKSPACE-TREE` is migrated; `make check-workflow-boundaries` requires every `.rs` file in `ui/sidebar/` to be declared by the row.

## Goals / Non-Goals

**Goals:**
- One reusable, bounded, typed-failure `win.reveal-workspace-path` action that drives the real sidebar navigation path, including the slice bin's request forwarding.
- Deterministic readiness for that action and bounded, unlabelled snapshot geometry for the proof runner.
- A `workspace-tree-reveal` scenario that proves, on real pixels, that the revealed row is drawn where the snapshot says, inside the viewport, contiguous with its neighbours, and at rest. For a row already on screen, the proof also shows that the header and the outer scroller did not move.
- Policy wiring so slice-bin-sensitive edits require this proof, as minimap edits require theirs.

**Non-Goals:**
- A user-visible "Reveal in Sidebar" menu item, palette command, or shortcut. The action is built so a later change can add one without changing behaviour; it is not added here.
- Changing the workspace scope or sidebar visibility on the user's behalf.
- Fixing the recorded slice-bin residuals (learning frame, two reconfigures) or the two-bin double-count. Those belong to `extend-closed-loop-geometry-verification`, and this scenario issues one request at a time.
- Python-oracle parity for the new scenario type.

## Decisions

### D1. `win.reveal-workspace-path`, string parameter, `diagnostic-only`, `dbus-action` surface

The catalog offers `ActionSurface::DbusAction` and `ExternalActivationSafety::DiagnosticOnly` ("exported for diagnostics but not currently a visible user command"). That is the honest classification until some UI invokes the action.

- *Alternative:* `contextual-user-command`, like `set-search-panel-query`. Rejected: that action mirrors a visible entry; this one has no visible counterpart yet.
- *Alternative:* an app-level action. Rejected: the sidebar is per-window, and window actions are what the runner activates.

The action is registered in `ui/window/actions.rs` and only delegates to the sidebar facade.

### D2. Pure resolution in `ui/sidebar/policy.rs`, coordination in a new `ui/sidebar/reveal_execution.rs`

Resolution is GTK-free: given the configured workspaces, the current scope, the visibility rule, and the target path, it returns either `(workspace, folder, ancestor chain)` or a typed failure (`not-in-workspace`, `outside-scope`, `hidden-by-visibility`). It belongs with the row's other pure decisions and inside the `ui/**/policy.rs` mutation scope.

Coordination is a stage order of its own, so it gets its own `execution` module: resolve → expand the next ancestor → await its admitted scan (resumption point) → repeat → select and `scroll_to` → await bin settle (resumption point) → terminal. `not-found` and `beyond-entry-cap` are only known after a scan, so they are classified in coordination from scan results.

The facade (`ui/sidebar/mod.rs`, 293 of 370 lines) narrates the stage order and its two resumption points. The narrative must fit the remaining headroom. The matrix row is re-derived for size and seam counts.

- *Alternative:* extend `folder_execution.rs`. Rejected: it is a called presentation surface, and the convention forbids hiding a new stage order there.

### D3. Reuse the section's scan path and `select_and_scroll_to`; never touch the outer adjustment

Each ancestor is expanded with `TreeListRow::set_expanded(true)`, the same call a user click makes. That goes through the bounded admission, the 10,000-entry cap, and the generation guard. The final step is `select_and_scroll_to`. The bin, not the reveal, moves the outer scroller.

A generation counter per window rejects stale continuations (`superseded`), and section disposal ends the reveal as `superseded`. This is the property that makes the scenario a proof of the forwarding path rather than of a shortcut.

### D4. Settle on bin evidence, with a bounded frame budget

Reveal readiness needs to know that the bin's forwarded request has been applied. Two alternatives were rejected:

- *Counting frames:* nondeterministic under a loaded headless compositor, which is the flake class `preexisting-blockers.md` forbids.
- *Polling the outer adjustment for stability:* passes during the gap between request and idle.

A read-only `ViewportSliceBin::outer_request_pending()` (backed by the existing `pending_outer` cell, `Some` exactly while the idle is scheduled) gives the exact state. It is a small, governed GTK Lush API addition: API snapshot, CHANGELOG, and an adoption-lab test. The reveal terminates `revealed` once all of these hold on one frame-clock tick:

- the row is mapped;
- the row is inside the outer viewport;
- the bin reports no pending request;
- the bin's deferred divergence is empty.

If that does not happen within a bounded budget of 60 frame ticks, the reveal terminates `unsettled`. A reveal that ends `unsettled` is a real finding, never retried.

### D5. Snapshot: status and geometry only, projected from evidence

The status, generation, and depth are added to `WorkspaceTreeEvidence` and projected into `window.workspace`. This follows the row's rule that the snapshot projects from the evidence surface, and `docs/automation-reference.md`'s Evidence Projection Map gains the rows.

Geometry comes from `visual_geometry_snapshot`:

- `workspace-sidebar-viewport` (the outer `GtkScrolledWindow`);
- `workspace-revealed-row` (the mapped row widget for the target);
- `workspace-revealed-section-header`;
- up to eight neighbours as unlabelled rectangles.

No file names cross the privacy boundary. The runner does not need them, because the fixture's own target path identifies the case.

### D6. Scenario: two manifests, snapshot relationships plus one pixel anchor

Protected regions are per manifest, and the header-unchanged claim only holds when nothing scrolls. So `already-visible` gets its own manifest (`workspace-tree-reveal-at-rest.json`), which protects the section header crop. Both manifests protect the header bar, the status bar, and the editor viewport.

Relationships come from the snapshot:

- inside the viewport;
- contiguous neighbours at zero tolerance (rows are whole-pixel allocations);
- at rest across two post-readiness snapshots;
- outer scroll and header unchanged for at-rest.

The one claim a snapshot cannot make is that the pixels are where the widget says they are. That is exactly the v0.8.1 defect, so a detector `workspace-selected-row-top-edge` finds the selected-row highlight inside the `workspace-sidebar` crop. The highlight is a band of uniform non-background colour spanning at least 60% of the crop width. The detector is checked against the snapshot row's top edge at `max_screen_y_delta: 0`.

Fixture kinds:

| Kind | Setup | Why |
|---|---|---|
| `beyond-cap` | 400 files, reveal the last | crosses `GTK_LIST_VIEW_MAX_LIST_ITEMS` |
| `nested-beyond-cap` | 300 entries two directories deep, reveal entry 250 | ancestor expansion |
| `two-workspaces` | reveal in the second section | chrome above the bin includes a whole first section |
| `already-visible` | reveal a row that is on screen at rest | the resting-bin claim |

The size matrix is desktop 1260×900 and short-wide 1738×342. Short-wide is where the header-loss defects appeared.

### D7. Policy: a third invariant ID, a narrow path set, mirrored

`WORKSPACE_TREE_ROW_ANCHORS_INVARIANT = "workspace-tree-rendered-row-anchors"` is required for the paths listed in the spec, and only those. The whole of `ui/sidebar/**` is deliberately excluded, so dialog and persistence edits do not demand a live session. Both policy implementations are updated in one task, and their self-tests prove parity.

## Risks / Trade-offs

- **The selection highlight colour differs per theme, and focus rings may add edges, so the detector could misfire.** → Mitigation: detector unit tests on corpus PNG fixtures for both forced colour schemes, generated from the first live run and frozen with the existing corpus replay. If a scheme cannot be detected reliably, the manifest declares the anchor for the scheme that can. The gap is recorded in the design outcome rather than loosening the delta.
- **The headless frame clock freezes after an idle-driven scroll.** This was observed 2026-09-14, and it is why the latest-target rewrite regressed. → Mitigation: the reveal runs in the real-session `visual-geometry-smoke` lane, not a headless one. Its widget tests use the existing `flush_after_delay` + readiness helpers, which already cope with that behaviour.
- **Facade headroom (77 lines) may be tight.** → Mitigation: the delegate-harder rule. If the narrative still does not fit, stop and escalate per the convention rather than overrunning 370.
- **A new blocker in `idle`/`visual-geometry-settled` could stall unrelated waits if a reveal never terminates.** → Mitigation: every path ends in a typed terminal (D4's budget, supersession, disposal), and a widget test proves disposal mid-reveal clears the blocker.
- **GTK Lush API growth.** → Mitigation: one read-only accessor, which matches the existing evidence accessors. Stewardship checks run in the same change.

## Migration Plan

This change is purely additive, so no migration is needed. To roll back, revert the change: no persisted format, GSettings key, or existing automation contract changes.

## Open Questions

None blocking. Whether a user-facing "Reveal in Sidebar" command should later promote the action to `contextual-user-command` is left to a future change.
