## 1. User-Facing Text

- [x] 1.1 Rename the Preferences > Editor > Appearance row title from `Transparency` to `Background Opacity` in the Blueprint source.
- [x] 1.2 Replace the row subtitle with text that explains lower values make editor and Markdown preview backgrounds more transparent.
- [x] 1.3 Regenerate the GtkBuilder UI and template contract so generated resources match the Blueprint source.
- [x] 1.4 Update the GSettings summary/description only where the text is user-facing or developer-facing documentation, without changing the `tab-content-opacity` key, range, default, or stored value meaning.

## 2. Documentation And Specs

- [x] 2.1 Update README and agent-facing guidance so the visible control is named `Background Opacity` while the feature can still be described as tab-content transparency.
- [x] 2.2 Keep implementation-oriented wording such as `tab-content-opacity` and opacity-aware style schemes intact where it describes internal behavior.
- [x] 2.3 Confirm the canonical `tab-content-transparency` spec will sync the new wording and slider semantics during archive.

## 3. Regression Coverage

- [x] 3.1 Update preference widget tests to assert the `Background Opacity` title, explanatory subtitle, and default `100%` opacity readout.
- [x] 3.2 Add or update coverage that a non-default value such as `85%` is displayed as opacity and is not inverted into a transparency percentage.
- [x] 3.3 Add or update tests that the slider still persists through `tab-content-opacity` and immediately updates editor and Markdown preview backgrounds.
- [x] 3.4 Update template-contract assertions so stale `Transparency` row text cannot silently return.

## 4. Validation

- [x] 4.1 Run `openspec validate clarify-tab-content-opacity-preference --strict`.
- [x] 4.2 Run `./scripts/blueprint-templates.sh check`.
- [x] 4.3 Run the focused preferences/window widget tests that cover the opacity preference.
- [x] 4.4 Run `cargo check -p lushtext-core --lib`.
- [x] 4.5 Run `git diff --check`.
