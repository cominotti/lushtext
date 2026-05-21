## 1. Baseline And Test Shape

- [x] 1.1 Inspect `resources/ui/window.ui`, `crates/lushtext-core/src/ui/window/`, and `crates/lushtext/tests/widget/window.rs` to identify every manual document-properties rehosting call site and every pane/sheet assertion.
- [x] 1.2 Add or update semantic widget-test helpers that answer whether document properties are visible and whether the active presentation is pane or sheet without requiring each test to know the low-level widget tree.
- [x] 1.3 Add regression tests for pane-to-sheet transition, sheet-to-pane transition, closed-state preservation, and presentation-independent focus restoration from the new `document-properties-pane` requirement.

## 2. Adaptive Layout Template

- [x] 2.1 Update `resources/ui/window.ui` to introduce an `AdwMultiLayoutView` with named slots for the primary editor shell and the document-properties surface.
- [x] 2.2 Define the spacious layout with an `AdwOverlaySplitView` that places the primary slot as content and the properties slot as the right-side sidebar.
- [x] 2.3 Define the compact layout with an `AdwBottomSheet` that places the primary slot as content and the properties slot as the sheet.
- [x] 2.4 Bind only the template children needed by the window adapter and avoid duplicating the properties panel as separate pane and sheet widgets.

## 3. Window Adapter Refactor

- [x] 3.1 Add named UI-layer presentation helpers, such as `PropertiesPresentation::Pane` and `PropertiesPresentation::Sheet`, or equivalent functions that remove direct reliance on collapsed-state booleans.
- [x] 3.2 Rewire the existing dynamic editor-width guard so breakpoint transitions select the document-properties layout presentation instead of manually rehosting `properties_panel`.
- [x] 3.3 Replace `rehost_document_properties_panel` with command-shaped synchronization code that opens or closes only the relevant right pane or bottom sheet for the active presentation.
- [x] 3.4 Preserve current requested/rendered secondary-surface state, workspace-sidebar mutual exclusion, Focus Mode suppression, action active-state synchronization, and focus restoration.
- [x] 3.5 Keep this as a GTK driving-adapter refactor: do not add model or service APIs, do not introduce trait wrappers for Libadwaita widgets, and extract a focused `ui/window` workflow module only if it improves navigation.

## 4. Robust Regression Coverage

- [x] 4.1 Update existing wide-layout tests so opening document properties still shows a right-side pane and keeps the editor content visible.
- [x] 4.2 Update existing compact-layout tests so opening document properties still shows a bottom sheet and closes the workspace sidebar when compact arbitration requires it.
- [x] 4.3 Keep coverage for the dynamic guard: default no-workspace width, default `Comfy` workspace width, `Large` workspace preset behavior, and hiding the workspace sidebar relaxing the properties breakpoint.
- [x] 4.4 Keep coverage for requested-state restoration when a compact transition temporarily suppresses one secondary surface and the window later widens.
- [x] 4.5 Add or update tests proving active-document properties do not become stale when the presentation changes between pane and sheet.
- [x] 4.6 Add or update tests proving focus restoration works when document properties close or are suppressed from both pane and sheet presentations.

## 5. Architecture And Verification

- [x] 5.1 Review the final implementation against `rust-hex-arch`: commands mutate GTK state and return narrow outcomes, queries only inspect layout/requested state, no domain/service leakage is introduced, and any extraction is workflow-oriented rather than generic.
- [x] 5.2 Run `cargo fmt --check`.
- [x] 5.3 Run `cargo check --workspace`.
- [x] 5.4 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.5 Run `make test-unit`.
- [x] 5.6 Run `make test-int`.
- [x] 5.7 Run `./scripts/run-widget-tests.sh --auto`.
- [x] 5.8 Run `cargo hakari verify`.
- [x] 5.9 Run `cargo deny check advisories bans sources`.
- [x] 5.10 Run `git diff --check`.
