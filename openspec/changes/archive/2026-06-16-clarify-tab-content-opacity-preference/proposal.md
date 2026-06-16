## Why

The current preference row says `Transparency` while showing the stored opacity percentage, so a value like `85%` can read as "85% transparent" even though the editor is only slightly transparent. Renaming the user-facing text around opacity semantics makes the control truthful without changing the feature's behavior.

## What Changes

- Rename the Preferences > Editor > Appearance row from `Transparency` to an opacity-centered label such as `Background Opacity`.
- Replace the subtitle with copy that explains lower values make editor and Markdown preview document backgrounds more transparent.
- Keep the existing slider semantics: `100%` remains fully opaque, lower values increase transparency, and the stored `tab-content-opacity` value remains an opacity value.
- Update user-facing docs, accessibility-visible text, generated template contract expectations, and tests that assert the row title/subtitle.
- Do not introduce a migration, reset existing preferences, or invert persisted values.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `tab-content-transparency`: clarify the preference's user-facing wording so its visible percentage is described as opacity, while preserving existing behavior and persistence semantics.

## Impact

- Affected UI templates: `resources/ui/preferences.blp` and generated `resources/ui/preferences.ui`.
- Affected settings/docs: GSettings summary/description, README, AGENTS guidance, and any user-facing references that name the preference.
- Affected tests/contracts: widget preference assertions, template-contract expectations, and the canonical `tab-content-transparency` spec.
- No dependency, storage-format, or runtime rendering changes are expected.
