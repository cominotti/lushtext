## Context

LushText currently loads thirteen GtkBuilder XML templates from `resources/ui/*.ui` through resource-backed Rust `CompositeTemplate` types. The same `.ui` files are bundled by two resource paths: `crates/lushtext-core/build.rs` for direct Cargo development builds, and `resources/meson.build` for Meson, Flatpak, and Snap builds. The templates include several historically fragile layout surfaces: the main window's nested split/layout shell, the search bar's grid overlays, editor inline alerts, sidebar workspace sections, search panel result scrollers, document properties, Markdown preview, menus, shortcuts, accessibility metadata, and custom Rust GTK widgets.

Blueprint is a build-time language that compiles to GtkBuilder XML. That makes it attractive as an authoring format, but it does not itself guarantee identical runtime geometry. The migration therefore needs to preserve the generated GtkBuilder resources and prove equivalence through structural checks, widget tests, and visual smoke coverage.

## Goals / Non-Goals

**Goals:**

- Make Blueprint (`.blp`) the reviewed source format for every existing UI template.
- Keep committed GtkBuilder `.ui` files as the runtime resource input so the app, Flatpak, and Snap continue loading the same resource paths.
- Ensure generated `.ui` output is deterministic and checked for drift against the `.blp` source.
- Preserve 1:1 UI/UX behavior: layout, geometry, spacing, text, translations, actions, menus, shortcuts, accessibility metadata, focus paths, CSS classes, object IDs, and template-child bindings.
- Add enough structural, widget, and real-session verification to catch geometry regressions that raw source review can miss.
- Document local and CI Blueprint tooling without adding a runtime dependency.

**Non-Goals:**

- Do not redesign any UI surface as part of the Blueprint migration.
- Do not change runtime widget code except where comments/tests/tooling need to reflect Blueprint-generated templates.
- Do not remove committed `.ui` files in this change.
- Do not change GSettings schemas, persisted data, application IDs, actions, shortcuts, or packaging identity.
- Do not treat byte-for-byte equivalence with the old hand-written XML as mandatory when the Blueprint compiler emits equivalent but normalized XML. Runtime and semantic equivalence are mandatory.

## Decisions

### 1. Keep `.ui` files committed and resource-backed

Blueprint files should live beside their generated templates, for example `resources/ui/window.blp` and `resources/ui/window.ui`. Rust `#[template(resource = ".../ui/window.ui")]` attributes and `resources/dev.cominotti.lushtext.gresource.xml` continue to reference `.ui` files.

This keeps direct Cargo builds, Meson builds, Flatpak builds, and Snap builds on the existing GResource contract. It also makes rollback simple: restore the previous `.ui` files and remove the new `.blp` source if the migration proves unsafe.

Alternative considered: generate `.ui` files only into build directories. That would remove committed generated files, but it would make every build path depend on `blueprint-compiler`, complicate Flatpak/Snap setup, and make compiler-version differences affect release output.

### 2. Treat `.blp` as source of truth and `.ui` as generated output

The implementation should add a regeneration command and a drift-check command. A typical flow is:

```text
resources/ui/*.blp
      │
      ▼ blueprint-compiler compile
resources/ui/*.ui
      │
      ▼ glib-build-tools / gnome.compile_resources
runtime GResource templates
```

Manual edits to generated `.ui` files should fail CI unless the matching `.blp` source is updated and regeneration produces the same output. Existing XML comments that explain geometry-sensitive choices should be moved into the `.blp` source where they remain useful.

Alternative considered: allow either `.blp` or `.ui` edits. That would make review ambiguous and would weaken the 1:1 fidelity guarantee because generated output could drift silently.

### 3. Use compiler output plus semantic audits for equivalence

The first conversion should use `blueprint-compiler decompile` or `blueprint-compiler port` as a starting point, then manually review and adjust each `.blp`. Because Blueprint compiler output may normalize formatting or ordering, the migration should verify equivalence at several levels:

- Generated `.ui` files match the committed generated output after regeneration.
- Every template class and parent class is preserved.
- Every Rust `TemplateChild` ID still appears in the generated `.ui` with the expected widget type.
- Every custom widget reference keeps the same object type and registration expectation.
- Every named menu, menu item action, shortcut surface, translatable string, accessibility property, CSS class, and object ID is preserved.
- Every layout-sensitive child role and layout property is preserved, including overlay/start/end/suffix/primary/properties child types, grid coordinates, `GtkPaned` shrink/resize flags, scroller propagation/policies, `GtkSizeGroup` members, `AdwLayoutSlot` IDs, and `AdwBottomSheet` content/sheet roles.

Alternative considered: rely on the app compiling. Compilation catches missing template children only when the relevant widget is instantiated and does not prove geometry, accessibility, or menu equivalence.

### 4. Convert low-risk templates first, high-risk templates last

The implementation should establish the conversion pattern on lower-risk templates before touching the historically geometry-sensitive ones. A reasonable order is:

1. `markdown-preview`, `status-bar`, `command-palette`, `editor-page`
2. `properties-panel`, `preferences`, `sidebar`, `workspace-section`
3. `search-panel`, `search-bar`, `info-bar`
4. `window`, `shortcuts`

Each batch should regenerate, drift-check, build, and run focused template/widget validation before moving on. `window`, `search-bar`, `info-bar`, and `workspace-section` deserve the deepest review because they carry nested layout roles, overlays, adaptive chrome, wrapping actions, or constrained geometry.

Alternative considered: convert all templates in one mechanical pass. That maximizes churn and makes it harder to isolate a geometry regression to one template.

### 5. Make visual and warning checks part of acceptance

The migration must run the existing widget harness and a real-session visual pass. Structural equivalence is necessary but not sufficient because previous issues involved GTK allocation, paned/revealer interactions, constrained widths, and live warnings.

Visual acceptance should include:

- main editor shell with tabs, status bar, workspace control, and document surface;
- compact/narrow layouts with workspace sidebar and document properties requested;
- short-window layouts where persistent chrome must remain visible;
- search bar and workspace search panel in empty and populated states;
- editor inline alerts in no-action, warning, error/retry, wide, and narrow states;
- sidebar with no workspaces, zero-folder workspaces, representative folders, many/long names, and constrained width;
- Markdown preview and document-properties surfaces;
- modal or popup surfaces covered by existing geometry-stability expectations.

Unexpected GTK, Libadwaita, GDK, renderer, accessibility, template-loading, or allocation warnings should fail acceptance.

### 6. Keep packaging impact tool-only

`blueprint-compiler` should be required for regeneration and drift checks, not for end-user runtime. Flatpak and Snap should continue shipping the generated `.ui` resources. CI and contributor setup should install or verify `blueprint-compiler` for the drift lane. If a packaging lane intentionally runs the drift check, it must install the tool in that lane; ordinary resource compilation should continue to work from committed `.ui` files.

Alternative considered: make `blueprint-compiler` a required Flatpak/Snap build input. That follows some GNOME project patterns, but it increases packaging risk without improving the 1:1 preservation guarantee for this migration.

## Risks / Trade-offs

- [Risk] Blueprint decompilation may not preserve a layout-sensitive detail. -> Mitigation: use decompile only as a starting point and require per-template structural audits plus widget/visual verification.
- [Risk] Committed generated `.ui` files can drift from `.blp` source. -> Mitigation: add a regeneration command and CI drift check that fails on mismatch.
- [Risk] Keeping generated `.ui` files increases diff size. -> Mitigation: accept the churn once for preservation and review generated output as machine output; future human edits happen in `.blp`.
- [Risk] Compiler-version differences may produce formatting churn. -> Mitigation: document the expected compiler source/version range and keep drift checks deterministic in CI.
- [Risk] Geometry regressions may pass structural checks. -> Mitigation: require widget allocation tests, real-session visual captures, and warning-log scans for geometry-sensitive states.
- [Risk] Contributor setup gets another tool. -> Mitigation: keep ordinary builds usable from committed `.ui` output and make the Blueprint tool needed only for template edits and CI drift checks.

## Migration Plan

1. Add Blueprint tooling documentation and a regeneration/check command that can run before any template conversion.
2. Convert low-risk templates and establish the generated-output format.
3. Add or update structural audits for template class, parent, object ID, child role, action/menu, accessibility, CSS class, and `TemplateChild` coverage.
4. Convert the remaining templates in risk-ranked batches, preserving `.ui` resource paths.
5. Run widget and visual verification after each high-risk batch.
6. Update README, AGENTS/rules, packaging guidance, and CI so contributors understand `.blp` source and generated `.ui` drift checks.
7. Archive only after the full validation ladder passes and the generated `.ui` output is warning-free in the representative live UI states.

Rollback is straightforward before release: revert the `.blp` additions, generated `.ui` changes, and tooling/docs updates. Because this change must not alter persisted data or runtime IDs, rollback does not require user-data migration.

## Open Questions

None.
