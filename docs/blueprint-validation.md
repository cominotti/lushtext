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

`./scripts/blueprint-templates.sh lint` is advisory. It runs
`blueprint-compiler lint`, groups diagnostics by rule and file, and fails only
when a lint rule is unclassified or reports an error.

## Compile Warning Policy

Unknown `blueprint-compiler compile` warnings fail `make check-blueprint`.

The only known warning class accepted today is the GTK 5 deprecation warning for
`Gtk.ShortcutsWindow`, `Gtk.ShortcutsSection`, `Gtk.ShortcutsGroup`, and
`Gtk.ShortcutsShortcut` in `resources/ui/shortcuts.blp`. That dialog intentionally
uses GTK's existing shortcuts window until a separate UI redesign replaces it.

## Advisory Lint Policy

| Rule | Status | Rationale |
| --- | --- | --- |
| `adjustment_prop_order` | Classified | The source is normalized to lower, upper, then value, but blueprint-compiler 0.20.4 still warns when increment properties are present. Removing increments would change control behavior. |
| `scrollable_parent` | Classified | Current findings involve custom composite templates and layout-owned children. Changes require widget or visual proof because scroll ownership affects geometry. |
| `use_adw_bin` | Classified | Single-child boxes may carry CSS classes, margins, or layout semantics. Container swaps require generated-UI and visual proof. |
| `translate_display_string` | Classified | Current findings include runtime-populated labels, symbolic toggle text, technical badges, and branding. Change them only during a localization-aware pass. |
| `use_unicode` | Classified | Ellipsis cleanup is user-visible text churn and should move with localization review. |
| `missing_descriptive_text` | Classified | The current image is decorative; accessibility semantics need widget-level verification before changing. |
| `avoid_all_caps` | Classified | `LF` and `UTF-8` are compact technical status labels, not prose labels. |

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
