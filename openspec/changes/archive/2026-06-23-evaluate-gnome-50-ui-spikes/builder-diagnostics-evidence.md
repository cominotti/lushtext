# GTK Builder Diagnostics Spike Evidence

Date: 2026-06-23

## Tool And Runtime Versions

| Tool | Result |
| --- | --- |
| `pkg-config --modversion gtk4` | `4.22.4` |
| `pkg-config --modversion libadwaita-1` | `1.9.1` |
| `pkg-config --modversion gtksourceview-5` | `5.20.0` |
| `blueprint-compiler --version` | `0.20.4` |
| `gtk4-builder-tool --version` | No version flag output; the command prints usage and command help. Treat the GTK runtime version above as the version anchor for the tool. |
| `rustc --version` | `rustc 1.96.0 (ac68faa20 2026-05-25)` |
| `cargo --version` | `cargo 1.96.0 (30a34c682 2026-05-25)` |

## Existing Baseline

The existing Blueprint lane remains authoritative:

- `make check-blueprint`: blocking source generation, generated XML drift, and
  `resources/ui/template-contract.json` validation.
- `make lint-blueprint`: curated advisory lint policy, including promoted
  accessibility lint for decorative or descriptive images.

During this spike, `make lint-blueprint` exposed actionable
`missing_descriptive_text` findings and a documented `scrollable_parent` policy
count drift. Those were fixed or re-baselined in the existing validation policy.

## Standalone `gtk4-builder-tool validate`

Command shape:

```sh
gtk4-builder-tool validate resources/ui/<template>.ui
```

Raw local artifacts are under `build/gnome-50-ui-spikes/builder-tool-after/`.

| Template | Standalone result | Classification |
| --- | --- | --- |
| `resources/ui/command-palette.ui` | pass | Useful but redundant with `make check-blueprint` for GTK-loadable templates. |
| `resources/ui/open-popover.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/search-bar.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/search-panel.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/shortcuts.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/sidebar.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/status-bar.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/workspace-section.ui` | pass | Useful but redundant with existing Blueprint checks. |
| `resources/ui/editor-page.ui` | `Invalid object type 'LushtextInfoBar'` | Known standalone limitation: app composite widget type is not registered. |
| `resources/ui/info-bar.ui` | `Invalid object type 'AdwWrapBox'` | Known standalone limitation: Libadwaita type is not initialized in the standalone tool context. |
| `resources/ui/markdown-preview.ui` | `Invalid object type 'AdwStatusPage'` | Known standalone limitation: Libadwaita type is not initialized. |
| `resources/ui/preferences.ui` | `Failed to lookup template parent type AdwPreferencesDialog` | Known standalone limitation: Libadwaita template parent is not initialized. |
| `resources/ui/properties-panel.ui` | `Invalid object type 'AdwPreferencesGroup'` | Known standalone limitation: Libadwaita type is not initialized. |
| `resources/ui/window.ui` | `Failed to lookup template parent type AdwApplicationWindow` | Known standalone limitation: Libadwaita template parent is not initialized. |

Standalone validation is useful as an exploratory probe, but it cannot replace
the existing generated-template checks because it cannot load every LushText
template without an initialized application context.

## Runtime Builder Diagnostics

Command shape:

```sh
GTK_DEBUG=builder,builder-objects \
  scripts/run-widget-tests.sh --headless -- <test> --exact --nocapture
```

Selected coverage:

| Probe | Test | State covered | Result after fixes |
| --- | --- | --- | --- |
| No-context startup | `window::test_split_view_defaults_restore_on_window` | Main shell with no active document context. | Test body passed; command status was nonzero only because the harness treats the host `GTK_DEBUG` warning as unexpected stderr. |
| Representative document properties | `window::test_properties_panel_updates_for_file_backed_editor` | Saved-file document, document-properties rows, shell templates. | Test body passed; same unsupported-host stderr classification. |
| Notes sidebar | `window::test_notes_browser_uses_sectioned_adw_sidebar_and_filters_note_body` | Lazy Notes browser with sectioned `AdwSidebar`. | Test body passed; same unsupported-host stderr classification. |
| Local History sidebar | `window::test_local_history_browser_controls_expose_accessibility_roles` | Lazy Local History browser with `AdwSidebar`. | Test body passed; same unsupported-host stderr classification. |

Initial runtime probes found actionable GtkBuilder deprecations:

- `<child> in GtkScrolledWindow is deprecated, just set the child property`
- `<child> in GtkRevealer is deprecated, just set the child property`
- `<child> in GtkOverlay is deprecated, just set the child property`

Owning sources fixed in the current spike:

- `resources/ui/editor-page.blp`
- `resources/ui/markdown-preview.blp`
- `resources/ui/search-bar.blp`
- `resources/ui/search-panel.blp`
- `resources/ui/sidebar.blp`
- `resources/ui/window.blp`
- `resources/ui/workspace-section.blp`

After regeneration, selected runtime probes no longer emit the `<child>`
deprecation diagnostics.

The remaining line is:

```text
GTK_DEBUG set but ignored because gtk isn't built with G_ENABLE_DEBUG
```

Classification: unsupported-host blocker for this debug channel, emitted by the
runtime environment. It is not an app template defect.

## Coverage Gaps

The selected runtime probes do not instantiate every lazy surface. Uncovered or
only standalone-validated areas include:

- Command palette.
- Open popover.
- Preferences dialog.
- Search bar and workspace search panel beyond generated-template validation.
- Markdown preview placeholder states.
- Shortcuts window.
- File-health review dialog.
- Encoding and line-ending dialogs.
- File chooser portal dialog.
- Destructive confirmations.

A future automated lane must name which of these are included, skipped, or
covered by separate commands.

## Enforcement Recommendation

Keep runtime builder diagnostics as a documented manual recipe for now.

Do not make it blocking yet because:

- Local GTK builds may ignore `GTK_DEBUG` unless built with `G_ENABLE_DEBUG`.
- Standalone `gtk4-builder-tool validate` cannot load Libadwaita or app
  composite templates without initialized types.
- The current widget harness treats any stderr as unexpected, so an advisory or
  blocking target needs classification logic first.

Future automation should be a separate proposal that provides:

- A GTK runtime with builder debug output enabled.
- A declared template/surface coverage map.
- Diagnostic classification for host noise versus app findings.
- A decision about manual, advisory, widget-test mode, smoke-test mode, or
  blocking `make check` integration.

## Product Boundary

This spike introduced no Cargo dependency, Flatpak permission, GSettings schema,
app-data format, automation API, or user-visible UI behavior changes. The only
source changes are template diagnostics/accessibility cleanup, generated XML,
template-contract regeneration, validation-policy documentation, and spike
evidence.
