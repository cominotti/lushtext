## 1. Tooling and Source Policy

- [x] 1.1 Decide the exact Blueprint compiler installation path for local Fedora/Toolbx use and CI, then document the expected `blueprint-compiler` command and version source.
- [x] 1.2 Add `resources/ui/*.blp` as the editable source pattern while keeping generated `resources/ui/*.ui` files committed as runtime GResource inputs.
- [x] 1.3 Add a deterministic regeneration command that compiles every `resources/ui/*.blp` file into its matching `resources/ui/*.ui` file.
- [x] 1.4 Add a drift-check command that regenerates templates into a temporary location or controlled worktree state and fails when committed `.ui` output is stale.
- [x] 1.5 Add a clear missing-tool failure message for regeneration and drift validation when `blueprint-compiler` is unavailable.
- [x] 1.6 Add CI coverage for the Blueprint drift check without making end-user runtime packages depend on `blueprint-compiler`.

## 2. Structural Equivalence Checks

- [x] 2.1 Add or update an audit that proves every Rust `#[template(resource = "...ui")]` path still resolves to a generated `.ui` file in the GResource manifest.
- [x] 2.2 Add or update an audit that compares Rust `TemplateChild` IDs against generated `.ui` object IDs and reports missing or incompatible bindings.
- [x] 2.3 Add or update structural checks for template class, parent class, custom widget type names, object IDs, CSS classes, translation markers, accessibility properties, menu IDs, action names, and shortcut definitions.
- [x] 2.4 Add or update layout-sensitive checks for child roles, grid row/column/spans, overlay roles, `AdwLayoutSlot` IDs, bottom-sheet content/sheet roles, paned flags, scroller policies, size groups, revealers, margins, expand flags, width requests, and height requests.
- [x] 2.5 Confirm existing `gtk4-builder-tool` or widget-harness validation limitations for Libadwaita/custom widgets and make the widget harness authoritative where standalone builder validation is insufficient.

## 3. Template Conversion

- [x] 3.1 Convert low-risk templates `markdown-preview`, `status-bar`, `command-palette`, and `editor-page` to `.blp`, regenerate `.ui`, and verify drift and structural checks after the batch.
- [x] 3.2 Convert medium-risk templates `properties-panel`, `preferences`, `sidebar`, and `workspace-section` to `.blp`, regenerate `.ui`, and verify drift and structural checks after the batch.
- [x] 3.3 Convert high-risk templates `search-panel`, `search-bar`, and `info-bar` to `.blp`, regenerate `.ui`, and verify drift, structural checks, and focused widget coverage after the batch.
- [x] 3.4 Convert the main shell `window` template and `shortcuts` template to `.blp`, regenerate `.ui`, and verify drift, structural checks, and focused widget coverage after the batch.
- [x] 3.5 Move geometry-sensitive XML comments into the `.blp` source so future reviewers see the layout rationale in the editable files.
- [x] 3.6 Ensure no generated `.ui` change represents an intentional user-visible UI/UX change unless a separate OpenSpec explicitly authorizes it.

## 4. Build and Packaging Integration

- [x] 4.1 Verify direct `cargo build` and `cargo run` resource compilation continue to consume committed `.ui` files through `crates/lushtext-core/build.rs`.
- [x] 4.2 Verify `make meson-build` continues to consume committed `.ui` files through `resources/meson.build` and the existing GResource manifest.
- [x] 4.3 Verify Flatpak builds continue to bundle generated `.ui` files without adding `blueprint-compiler` as an end-user runtime dependency.
- [x] 4.4 Verify Snap validation/build guidance continues to consume generated `.ui` files and documents the Blueprint drift check as tooling-only.
- [x] 4.5 Update Flatpak or CI cache keys and setup steps if Blueprint source or drift-check scripts become build validation inputs.

## 5. Widget and Visual Fidelity Verification

- [x] 5.1 Run focused widget tests that instantiate every migrated `CompositeTemplate` and assert required child bindings, action widgets, visible defaults, and sensitive defaults.
- [x] 5.2 Run geometry-focused widget tests for compact, narrow, and short-window layouts with workspace sidebar and document properties requested.
- [x] 5.3 Run focused widget tests for search bar, workspace search panel, command palette, sidebar/workspace section, document properties, Markdown preview, and inline alert wide/narrow states.
- [x] 5.4 Run modal, popup, menu, and shortcuts coverage so menu models, shortcut surfaces, focus restoration, and popup geometry remain equivalent.
- [x] 5.5 Run a real-session visual smoke pass covering the main shell, compact/narrow/short layouts, search, sidebar state extremes, inline alerts, document properties, Markdown preview, and representative modal or popup surfaces.
- [x] 5.6 Scan runtime logs from widget and real-session runs and fail the change on new GTK, Libadwaita, GDK, renderer, accessibility, template-loading, or allocation warnings.

## 6. Documentation and Guidance

- [x] 6.1 Update README contributor guidance with the `.blp` source-of-truth policy, regeneration command, drift-check command, and missing-tool setup notes.
- [x] 6.2 Update root and nested agent guidance/rules that still describe `resources/ui/*.ui` as hand-authored GTK XML.
- [x] 6.3 Update build and packaging guidance to explain that committed `.ui` files remain the runtime resource input and `blueprint-compiler` is only a generation/check tool.
- [x] 6.4 Add a review note or checklist for future UI template edits requiring `.blp` edits, regenerated `.ui`, drift validation, and geometry-sensitive verification when relevant.

## 7. Final Validation

- [x] 7.1 Run Blueprint regeneration and drift validation from a clean worktree state.
- [x] 7.2 Run formatting or whitespace checks for generated resources, scripts, docs, and OpenSpec artifacts.
- [x] 7.3 Run the relevant non-widget Rust tests for resource or helper scripts touched by the change.
- [x] 7.4 Run the full relevant widget-test harness for migrated UI templates and geometry-sensitive surfaces.
- [x] 7.5 Run `make meson-build` and the Flatpak validation/build lane relevant to resource packaging.
- [x] 7.6 Run the visual smoke lane and preserve artifacts/logs that prove 1:1 UI/UX fidelity for the representative states.
- [x] 7.7 Run `openspec validate --change adopt-blueprint-ui-templates --strict`, `openspec validate --changes --strict`, and `git diff --check`.
