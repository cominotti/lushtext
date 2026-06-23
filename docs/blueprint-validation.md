# Blueprint Validation

Blueprint files in `resources/ui/*.blp` are the editable UI template source.
Generated GtkBuilder XML files in `resources/ui/*.ui` stay committed because
Cargo, Meson, Flatpak, and Snap resource builds consume XML at runtime.

Generated proof output belongs under ignored `build/` artifact paths. Keep
reviewable evidence in bounded text summaries such as
`build/blueprint-visual-diff/report.md`; do not commit screenshots, raw logs, or
pixel-diff images.

## Commands

```sh
make blueprint-generate
make check-blueprint
./scripts/blueprint-templates.sh lint
./scripts/compare-blueprint-visuals.sh --baseline-ref <commit-or-ref>
```

`make check-blueprint` is blocking. It compiles every `.blp`, checks generated
`.ui` drift, and runs the generated UI template contract audit.

`./scripts/blueprint-templates.sh lint` is a curated advisory gate. It runs
`blueprint-compiler lint`, groups diagnostics by rule and file, and fails when a
lint rule is unclassified, reports an error, a promoted must-stay-clean rule
regresses, or an accepted advisory finding exceeds its documented file/count
ceiling.

## Runtime Builder Diagnostics

For automated GtkBuilder diagnostics after GTK, Libadwaita, GResource, and
LushText composite widget types are initialized, use the dedicated smoke lane:

```sh
make builder-diagnostics-smoke
```

The target writes raw logs, command lines, runtime metadata, standalone
`gtk4-builder-tool` results, runtime probe results, `coverage.json`,
`findings.json`, `summary.json`, and `summary.md` under
`build/smoke/builder-diagnostics` by default. It scopes
`GTK_DEBUG=builder,builder-objects` to the diagnostics process only.

Provider modes:

```sh
LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER=auto make builder-diagnostics-smoke
LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER=host make builder-diagnostics-smoke
LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER=container make builder-diagnostics-smoke
```

`auto` prefers a host GTK build that proves `GTK_DEBUG` support, then falls
back to the reusable debug GTK container image when `podman` or `docker` is
available. CI uses the pinned prebuilt image
`ghcr.io/cominotti/lushtext-builder-diagnostics-runtime:gnome-50-debug`; that
image is built by `.github/workflows/builder-diagnostics-runtime.yml`, not by
the diagnostics lane itself. Override the image or runner with
`LUSHTEXT_BUILDER_DIAGNOSTICS_IMAGE` and
`LUSHTEXT_BUILDER_DIAGNOSTICS_CONTAINER`.

Some host GTK builds print `GTK_DEBUG set but ignored because gtk isn't built
with G_ENABLE_DEBUG`; treat that as an unsupported-runtime limitation for this
diagnostic channel, not as a template defect. Standalone
`gtk4-builder-tool validate` remains useful for GTK-loadable generated
templates, but it cannot load every LushText template because Libadwaita
template parents and app composite widget types are not registered in the
standalone context. The automated lane records those as known standalone-tool
limitations, then relies on initialized runtime probes for actionable evidence.

## Compile Warning Policy

Unknown `blueprint-compiler compile` warnings fail `make check-blueprint`.

The only known warning class accepted today is the GTK 5 deprecation warning for
`Gtk.ShortcutsWindow`, `Gtk.ShortcutsSection`, `Gtk.ShortcutsGroup`, and
`Gtk.ShortcutsShortcut` in `resources/ui/shortcuts.blp`. That dialog intentionally
uses GTK's existing shortcuts window until a separate UI redesign replaces it.

## Advisory Lint Policy

| Rule | Status | Accepted Ceiling | Rationale |
| --- | --- | --- | --- |
| `use_unicode` | Promoted | 0 | Visible labels use Unicode punctuation such as ellipsis. Reintroducing ASCII `...` is a policy regression. |
| `missing_descriptive_text` | Promoted | 0 | Images need descriptive text or `accessible-role: presentation` when decorative. |
| `translate_display_string` | Partially classified | `info-bar.blp` x2, `search-panel.blp` x4, `status-bar.blp` x3, `window.blp` x2 | Static user-facing strings that were safe to translate are fixed. Remaining findings are runtime-populated empty alert labels, symbolic search toggles or `.gitignore`, technical status tokens, and the LushText brand title. |
| `adjustment_prop_order` | Classified | `preferences.blp` x4 | The source is normalized to lower, upper, then value, but blueprint-compiler 0.20.4 still warns when increment properties are present. Removing increments would change control behavior. |
| `avoid_all_caps` | Classified | `status-bar.blp` x2 | `LF` and `UTF-8` are compact technical status labels, not prose labels. |
| `scrollable_parent` | Classified | `editor-page.blp` x2, `window.blp` x8 | Current findings involve custom composite templates and layout-owned children. Changes require widget or visual proof because scroll ownership affects geometry. |
| `use_adw_bin` | Classified | `info-bar.blp` x1, `search-panel.blp` x1, `status-bar.blp` x1, `window.blp` x1 | Single-child boxes carry CSS classes, Rust template-child bindings, animation state, or layout semantics. Container swaps require generated-UI and visual proof. |

## Visual Proof

Use `scripts/compare-blueprint-visuals.sh` for Blueprint template reviews that
need before/after proof:

```sh
./scripts/compare-blueprint-visuals.sh \
  --baseline-ref origin/main \
  --artifact-dir build/blueprint-visual-diff
```

The script captures the baseline and current checkout with the same fixtures,
state matrix, and viewport matrix, then writes `report.md` plus disposable image
artifacts. A zero-diff report supports a 1:1 UI/UX claim. Any nonzero diff must
be explained as intentional or treated as a validation failure.
