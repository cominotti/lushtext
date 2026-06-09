## Why

LushText's GTK UI is currently maintained as roughly two thousand lines of GtkBuilder XML across thirteen resource templates, which makes layout-sensitive reviews harder than they need to be. GNOME Blueprint can make those templates easier to read and edit, but the migration is only acceptable if the generated GtkBuilder output preserves the existing UI and geometry one-for-one.

## What Changes

- Adopt GNOME Blueprint as the source format for every existing `resources/ui/*.ui` template while keeping GtkBuilder `.ui` output as the runtime resource format.
- Add deterministic Blueprint regeneration and drift validation while keeping committed GtkBuilder `.ui` output as the runtime resource input for development Cargo builds and Meson-driven installed, Flatpak, and Snap builds.
- Preserve all existing `CompositeTemplate` resource paths, `TemplateChild` bindings, custom widget references, menu models, translations, accessibility metadata, shortcuts, and CSS hooks.
- Add generated-template drift checks so committed or generated `.ui` output cannot silently diverge from the `.blp` source.
- Add structural, widget, and visual verification that proves the migration is a 1:1 UI/UX preservation change, including geometry-sensitive compact, narrow, short-window, sidebar, search, inline-alert, properties, Markdown preview, and modal states.
- Document Blueprint tooling expectations for contributors, CI, Flatpak, and Snap packaging.

## Capabilities

### New Capabilities

- `ui-template-source-fidelity`: Defines the contract for Blueprint-authored UI templates, deterministic GtkBuilder generation, resource-path preservation, template-child equivalence, and warning-free 1:1 layout/geometry verification.

### Modified Capabilities

None.

## Impact

- Affected UI resources: all templates under `resources/ui/`, the GResource manifest at `resources/dev.cominotti.lushtext.gresource.xml`, and any generated `.ui` output policy.
- Affected build code: `crates/lushtext-core/build.rs`, `resources/meson.build`, `meson.build` or `meson_options.txt` if needed, `Makefile` helper targets, Flatpak manifest/build validation, Snap build validation, and CI cache keys or setup steps that depend on resource inputs.
- Affected Rust code: only comments or tests around resource-backed `CompositeTemplate` loading should need updates; runtime widget code must continue loading the same `.ui` resource paths.
- Affected docs and rules: README, root and UI/build agent guidance, and any contributor setup notes for `blueprint-compiler`.
- Dependency impact: adds `blueprint-compiler` as a contributor and CI generation/check tool only. No runtime GTK, Libadwaita, GSettings, persisted-data, or application behavior change is expected.
