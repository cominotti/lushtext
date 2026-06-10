## MODIFIED Requirements

### Requirement: Layout-sensitive GtkBuilder semantics are preserved
The generated `.ui` templates SHALL preserve every layout-sensitive GtkBuilder
semantic from the current UI unless a behavior-changing OpenSpec explicitly
authorizes that semantic to change. This includes child roles, layout
properties, grid coordinates, overlay layers, Adwaita layout slots,
bottom-sheet content and sheet roles, remaining paned shrink and resize flags,
scrolled-window propagation and scrollbar policies, size-group members,
revealer transition settings, visible and sensitive defaults, and custom widget
placement. For this change, the main-window Markdown preview shell is
intentionally authorized to replace the preview `GtkPaned` node with an
Adwaita-native preview presentation while preserving unrelated template
semantics.

#### Scenario: Nested shell layout keeps approved roles
- **WHEN** the main window template is generated from Blueprint after the preview-shell migration
- **THEN** `AdwMultiLayoutView`, `AdwLayoutSlot`, `AdwBottomSheet`, `AdwOverlaySplitView`, `GtkOverlay`, sidebar, properties, status bar, command palette, focus-mode, search-panel, and Markdown preview nodes preserve their approved role relationships
- **AND** the Markdown preview presentation preserves editor-content and end-secondary-surface roles without requiring a `GtkPaned#preview_paned` node
- **AND** compact, pane, sheet, overlay, primary, properties, content, sidebar, start-child, end-child, content-child, and sidebar-child placements remain equivalent except for the approved preview-shell replacement

#### Scenario: Grid and overlay controls keep their allocations
- **WHEN** the search bar and editor templates are generated from Blueprint
- **THEN** grid row, column, and span properties remain equivalent
- **AND** overlay children keep the same overlay roles and alignment properties

#### Scenario: Size and scroll contracts remain unchanged
- **WHEN** generated templates are structurally audited
- **THEN** `GtkSizeGroup` members, `GtkScrolledWindow` propagation, scrollbar policies, minimum content sizes, width requests, height requests, margins, and expand flags remain equivalent
- **AND** no unintended horizontal scrollbar, fake row, clipped persistent chrome, or unrelated context dependency is introduced

#### Scenario: Preview template contract is regenerated deliberately
- **WHEN** the preview shell replaces `GtkPaned#preview_paned`
- **THEN** Rust `TemplateChild` fields, generated `.ui` output, and `template-contract.json` are updated together
- **AND** the template drift and contract checks fail if stale paned-node expectations remain after the migration
