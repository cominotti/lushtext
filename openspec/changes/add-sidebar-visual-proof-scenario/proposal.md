## Why

The workspace sidebar is the surface where the most expensive geometry bugs of 2026 happened: the ~200-row `GtkListView` realized cap, the resting-bin forced scroll (v0.7.0), the forwarded settle, and the 1–3px content-box offset (v0.8.1). All of them are covered by headless widget tests, yet `cargo-gtk-proof` has **no sidebar or `ViewportSliceBin` scenario**, so none of that behaviour is proved against real rendered pixels in a real session. The screenshot lane cannot reach the state: under headless Mutter `atspi-key` never reaches the toplevel, tree rows expose no AT-SPI actions, top-level folder menus lack Focus Folder, and opening a file does not reveal its row. The formal-verification programme records this as an open deferral (`docs/next/formal-verification.md` §4, `formal-verification-next.md` N10) blocked on a `reveal-workspace-path` automation action.

## What Changes

- Add a catalog-registered window action `win.reveal-workspace-path` (string parameter: an absolute path inside a configured workspace folder). It expands the ancestor chain through the sidebar's existing bounded, generation-guarded scan path, selects the row, and scrolls it into view through the list's own `scroll_to`, so the `ViewportSliceBin` request forwarding is exercised for real. Paths that cannot be revealed end in a typed failure (not in a workspace, outside the current scope, not found, hidden by the visibility rule, beyond the per-directory entry cap, superseded, unsettled) instead of a silent no-op.
- Add a readiness blocker `workspace-path-reveal`, a readiness predicate `workspace-path-revealed` (reporting `workflow-failure` on a typed failure), a matching workflow event, bounded `window.workspace` reveal snapshot fields projected from the sidebar's evidence surface, and bounded sidebar visual-geometry surfaces (sidebar viewport, the revealed row, its section header, and at most eight neighbouring mapped rows).
- Add a read-only `ViewportSliceBin` evidence accessor reporting whether a forwarded outer-scroll request is still pending, so reveal readiness settles on the bin's own state rather than on elapsed frames.
- Add a `workspace-tree-reveal` scenario type to `cargo-gtk-proof` and two manifests under `scripts/visual-geometry-scenarios/`: one scrolling matrix (row beyond the 200-row cap, nested beyond the cap, second of two workspaces) and one at-rest matrix (the target is already visible, so the outer scroller and the section header must not move). Assertions combine snapshot geometry relationships with a screenshot-derived pixel anchor on the selected-row highlight.
- Add the invariant `workspace-tree-rendered-row-anchors` to the visual proof policy (Rust-authoritative `crates/cargo-gtk-proof/src/policy.rs`, mirrored by `scripts/check-visual-proof-policy.py`) and require it for changes to the slice bin, its pure geometry, the workspace section's list host, and the new scenario files.
- Update automation, end-user coverage, workflow-matrix, and formal-verification planning docs; close the deferral.

No breaking change: every existing action, predicate, snapshot field, and scenario keeps its name and meaning. Existing aggregate predicates (`visual-geometry-settled`, `accessibility-settled`, `idle`) gain the new blocker.

## Capabilities

### New Capabilities

None. Every requirement extends an existing capability.

### Modified Capabilities

- `dbus-automation-spine`: ADDED requirements for the `reveal-workspace-path` action and its typed failures, its readiness blocker/predicate and workflow event, and bounded sidebar reveal snapshot and visual-geometry fields.
- `cargo-gtk-proof`: ADDED requirement for the `workspace-tree-reveal` scenario type (schema validation, fixture preparation, primary action, relationship assertions) and its policy wiring.
- `visual-geometry-invariants`: ADDED requirements for the sidebar rendered-row invariant (scrolling and at-rest matrices, pixel anchor) and for policy-required sidebar proof on slice-bin-sensitive changes.
- `gtk-lush-viewport-slice`: ADDED requirement for the read-only pending-outer-request evidence accessor.
- `workspace-tree-virtualized-rendering`: ADDED requirement that a revealed path reaches every row through the same bounded paths as user navigation.

## Impact

- **Code**: `crates/lushtext-core/src/ui/sidebar/` (new `reveal_execution.rs` coordination module, pure resolution in `policy.rs`, fields in `evidence.rs`), `workspace_section/folder_execution.rs` (reuses `select_and_scroll_to`/`pending_selection`), `ui/window/actions.rs` (action registration), `services/action_catalog/mod.rs`, `ui/automation.rs` + `model/automation.rs` (readiness, snapshot, surfaces), `crates/gtk-lush/widgets/src/viewport_slice_bin/` (one accessor), `crates/cargo-gtk-proof/src/{model,live,runner,png,policy}.rs`, `scripts/check-visual-proof-policy.py`, `scripts/visual-geometry-scenarios/workspace-tree-reveal*.json`.
- **Tests**: failing-first widget tests in `crates/lushtext/tests/widget/` for the action, the typed failures, and readiness; action-catalog unit tests; `cargo-gtk-proof` schema, relationship, detector, and policy unit tests; automation-client self-test case; `gtk-lush-widgets` API snapshot.
- **Docs**: `docs/automation.md`, `docs/automation-reference.md`, `docs/end-user-coverage.md`, `docs/workflow-readability-matrix.md` (`WFR-WORKSPACE-TREE`), GTK Lush widgets README/CHANGELOG, `docs/next/formal-verification.md`, `docs/next/formal-verification-next.md`, `AGENTS.md` module layout.
- **Gates**: `make check-automation-docs`, `make automation-client-self-test`, `make check-workflow-boundaries`, `make check-visual-proof-policy`, `make check-gtk-lush-policy`, `make gtk-lush-public-api-advisory`, `make visual-geometry-smoke`, `make test-widget`, `make check`.
- **Dependencies**: none.
