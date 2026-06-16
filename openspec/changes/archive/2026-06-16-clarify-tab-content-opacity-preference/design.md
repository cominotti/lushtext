## Context

LushText already stores this preference as `tab-content-opacity`, with `1.0` as fully opaque and lower values making the editor and Markdown preview document backgrounds more transparent. The current preferences row title is `Transparency`, while the row suffix shows the stored opacity percentage. That creates a semantic mismatch: `85%` is technically "85% opacity" but can read as "85% transparency."

The feature is otherwise healthy: the slider is already backed by an opacity value, the rendering path uses opacity-aware document backgrounds rather than fading widget content, and non-document chrome stays opaque. This change is therefore a wording and contract clarification, not a rendering redesign.

## Goals / Non-Goals

**Goals:**

- Make the Preferences row label and subtitle describe the displayed percentage as opacity.
- Preserve the existing `tab-content-opacity` setting, value range, default, binding, and persistence behavior.
- Keep `100%` as fully opaque and lower values as more transparent.
- Update tests, template contracts, user docs, and specs that assert the old visible text.
- Keep the preference understandable at default, non-default, and constrained dialog widths.

**Non-Goals:**

- Invert stored values or show "transparency percent" in the UI.
- Migrate or reset existing user settings.
- Change rendering behavior, style-scheme cache behavior, transparency bounds, or slider step sizes.
- Rename internal Rust fields, helper functions, or the GSettings key unless needed for a user-facing assertion.

## Decisions

1. Label the row as `Background Opacity`.

   `Background Opacity` is short enough for the existing AdwActionRow and accurately describes the percentage suffix. It also avoids implying that editor text, the minimap, or window chrome become translucent.

   Alternative considered: `Document Background Opacity`. This is more precise but heavier in the compact Preferences list. The subtitle can carry the editor/Markdown preview scope instead.

2. Keep the slider as opacity, not transparency.

   The existing left-to-right slider maps directly to the stored value: lower means less opaque, higher means more opaque, `100%` means fully solid. Keeping that mapping avoids a migration, avoids inverted bindings, and matches the GSettings description.

   Alternative considered: show `Transparency 15%` for the same stored value. That is conceptually valid, but it creates an inversion layer in the UI and makes the default read as `0%`, which can look like the feature is off rather than fully opaque.

3. Make the subtitle teach the direction of change.

   The row title can stay compact if the subtitle says that lower values make editor and Markdown preview backgrounds more transparent. This resolves the only remaining ambiguity without adding visible instructional chrome elsewhere.

4. Preserve internal terminology where it describes implementation.

   Existing names such as `tab-content-opacity`, derived `lushtext-opacity-*` style schemes, and helper functions can remain opacity-based. The canonical spec can still describe the capability as tab-content transparency because the feature's product outcome is transparent document backgrounds.

## Risks / Trade-offs

- [Risk] Users who previously looked for `Transparency` may not immediately recognize the renamed row. -> Mitigation: keep the row in the same Preferences > Editor > Appearance location with the same percentage suffix and popover interaction.
- [Risk] `Background Opacity` could be misread as applying to the whole app background. -> Mitigation: the subtitle and tests MUST scope it to editor and Markdown preview document backgrounds.
- [Risk] Documentation can drift between "transparency" feature naming and "opacity" control naming. -> Mitigation: specs and docs should use "tab-content transparency" for the feature and "Background Opacity" for the visible control.
- [Risk] Translated template strings and template-contract baselines may catch only part of the rename. -> Mitigation: update Blueprint source, generated UI, template contract, and widget assertions together.

## Migration Plan

No data migration is required. Existing `tab-content-opacity` values remain valid and keep their current meaning.

Rollback is also simple: revert the text/spec/test changes without touching stored settings or derived style-scheme caches.

## Open Questions

None. The preferred label is `Background Opacity`; implementation can revisit only if the row proves too ambiguous in visual review.
