## 1. Presentation Rule

- [x] 1.1 Add or update the workspace-section row icon selection so the synthetic root row labeled `Files` uses a symbolic icon, preferably `view-list-symbolic`.
- [x] 1.2 Keep real directory rows, including drill-down focused roots, on the existing regular themed `folder` icon path.
- [x] 1.3 Keep file rows on the existing regular content-type icon path and fallback behavior.

## 2. Row Binding

- [x] 2.1 Tie the synthetic-root icon override to the existing display-name condition that produces the `Files` label.
- [x] 2.2 Preserve existing expansion, selection, refresh reconciliation, file peek, context menu, inline rename, and drill-down behavior.
- [x] 2.3 Avoid new dependencies, icon assets, settings, or theme-specific branches.

## 3. Tests

- [x] 3.1 Update the single-directory root-row widget coverage to assert the `Files` row uses the symbolic icon and keeps the `Files` label.
- [x] 3.2 Keep or add coverage proving drill-down root rows use the regular folder icon.
- [x] 3.3 Keep or add coverage proving file rows use regular content-type icons and do not inherit the synthetic root icon.

## 4. Verification

- [x] 4.1 Run `openspec validate symbolic-files-root-icon`.
- [x] 4.2 Run targeted workspace-section tests that cover the root, drill-down, and file-row icon presentation.
- [x] 4.3 Run the repo's required final check command if the apply phase changes Rust code.
