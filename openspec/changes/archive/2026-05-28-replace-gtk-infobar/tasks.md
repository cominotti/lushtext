## 1. Replace the Deprecated Widget Structure

- [x] 1.1 Replace `GtkInfoBar` objects in `resources/ui/info-bar.ui` with one `GtkRevealer` containing a supported inline alert row.
- [x] 1.2 Keep title, body, retry, discard, save, and dismiss controls addressable as template children.
- [x] 1.3 Preserve the editor-page placement of `LushtextInfoBar` above the editor content.

## 2. Update the Widget Adapter

- [x] 2.1 Update `crates/lushtext-core/src/ui/info_bar/imp.rs` template children to use supported GTK widgets only.
- [x] 2.2 Keep signal closures thin by routing retry, discard, save, and dismiss button activation through the existing stored callbacks.
- [x] 2.3 Update `crates/lushtext-core/src/ui/info_bar/mod.rs` so `render_notification` drives one reusable alert row, sets warning/error style classes, updates labels, and hides unused action buttons.
- [x] 2.4 Pair reveal state with widget visibility so cleared or dismissed alerts do not leave stale focus, accessibility, or layout residue.
- [x] 2.5 Remove `GtkInfoBar`-only deprecation expectations and comments.

## 3. Restore Visual Styling

- [x] 3.1 Add global CSS for the inline alert row in `resources/style/style.css` using Adwaita warning and error CSS variables.
- [x] 3.2 Preserve readable wrapping for titles, body text, and action button labels at narrow editor widths.
- [x] 3.3 Keep warning primary and secondary actions balanced when both are visible.

## 4. Update Tests

- [x] 4.1 Update editor-page widget tests that currently inspect `GtkInfoBar` internals to assert the supported replacement state.
- [x] 4.2 Add or update a widget test proving `LushtextInfoBar` no longer contains or instantiates `GtkInfoBar`.
- [x] 4.3 Preserve tests for warning actions, error retry, Save As visibility, normalize-line-endings routing, and dismiss behavior.
- [x] 4.4 Add coverage that dismissing one editor inline alert does not clear another editor's alert.

## 5. Documentation and Verification

- [x] 5.1 Update `AGENTS.md` and any nested/rule docs that describe `GtkInfoBar` as the current implementation.
- [x] 5.2 Check whether `README.md` mentions infobars or this widget pattern, and update it if needed.
- [x] 5.3 Run `openspec validate replace-gtk-infobar --strict`.
- [x] 5.4 Run focused widget tests for editor inline alerts.
- [x] 5.5 Run `make check`.
- [x] 5.6 Run `make test-widget-headless` or `make test` before final acceptance.
- [x] 5.7 Exercise the alert workflows with `make run` and confirm stderr is free of GTK, GLib, and pixman warnings.
