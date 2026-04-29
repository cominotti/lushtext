## Context

The completed `regular-themed-file-tree-icons` change made actual file-tree content use regular themed GNOME/libadwaita icons. Its current implementation also makes the single-directory workspace root row labeled `Files` use the regular `folder` icon.

That `Files` row is not the filesystem directory's real basename. It is a synthetic workspace-section landmark that groups the section's root content. The surrounding workspace controls use symbolic icons, while actual child directories and files now use regular themed icons. This correction keeps that semantic boundary visible.

## Goals / Non-Goals

**Goals:**

- Render the synthetic root row labeled `Files` with a symbolic workspace/navigation icon.
- Keep real directories, including drill-down focused roots that show the actual directory name, on the existing regular folder icon.
- Keep file rows on existing regular content-type icon rules.
- Preserve every existing row interaction and layout behavior.

**Non-Goals:**

- Do not revisit the broader decision to use regular themed icons for real file-tree content.
- Do not change workspace selector, header, refresh, replace-root, or focus-folder control icons.
- Do not add icon assets, dependencies, settings, or theme-specific overrides.

## Decisions

### Synthetic `Files` root is sidebar structure

The row factory should treat the normal single-directory root row labeled `Files` as sidebar structure for icon selection. It should render a symbolic icon, preferably `view-list-symbolic`, while preserving the current label and behavior.

Alternative considered: keep `folder` for visual consistency with directory rows. That makes the row look like an actual folder named `Files`, which is misleading because the label is an alias. The symbolic icon better communicates that this row is a workspace landmark.

### Real content rows keep regular themed icons

Actual directory and file rows should continue through the existing regular icon helper. Drill-down focused roots are included in this rule because they display a real focused folder name and act like the visible root of actual filesystem content.

Alternative considered: make all root rows symbolic. That would make drill-down roots inconsistent with their real-directory meaning and weaken the existing colorful content model.

### Keep the override close to display-name logic

Implementation should apply the synthetic-root icon override alongside the existing logic that decides when the root row displays `Files`. This keeps the exceptional visual rule tied to the same condition that creates the synthetic label.

Alternative considered: infer the special case only inside generic icon classification. That would blur a workspace presentation concern into content-type classification and make it easier to accidentally affect real directories.

## Risks / Trade-offs

- [Risk] The root row becomes visually different from its child folders. -> Mitigation: this is intentional because the row is not an actual child folder; tests should name the semantic distinction.
- [Risk] A broad helper change could make drill-down roots symbolic too. -> Mitigation: add or keep targeted coverage proving drill-down roots keep the regular folder icon.
- [Risk] A theme may not expose `view-list-symbolic`. -> Mitigation: use GTK's normal themed icon resolution and keep the correction limited to the existing icon slot; the implementation can retain the normal symbolic fallback behavior.

## Migration Plan

No data migration is required. This is a presentation-only change in the workspace-section row binding. Rollback is limited to restoring the previous folder icon selection for the synthetic root row.

## Open Questions

None.
