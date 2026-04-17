# Dual Sidebars: Workspace + Properties

## Status: Proposed

## Current Note

This note is still useful as the historical justification for moving the shell
onto toolkit-owned split views, but the shipped contract has moved beyond the
original proposal:

- the workspace sidebar still uses `AdwOverlaySplitView`
- the document-properties surface now adapts between a wide right pane and a
  compact `AdwBottomSheet`
- the header bar owns the `Document Properties` toggle with `info-outline-symbolic`
  and `F9`
- the bottom bar now keeps only quick editor state such as `EditorConfig`,
  line endings, and encoding; slower document details live in document
  properties

## Summary

Replace the current custom `GtkPaned`-driven left sidebar animation with a
split-view architecture that supports both:

- a left workspace sidebar
- a right info/properties sidebar modeled after GNOME Text Editor

The recommended design is **nested `AdwOverlaySplitView`s**:

- an **outer** split view for the left workspace sidebar
- an **inner** split view for the right properties pane

This intentionally trades away arbitrary draggable sidebar widths in favor of a
layout model that is smooth, adaptive, and toolkit-owned across desktop and
narrow widths.

## Current State

LushText currently has one left sidebar implemented as:

- `GtkPaned` for workspace/content width allocation
- manual `AdwTimedAnimation` updates of `GtkPaned::position`
- a `GtkRevealer` wrapper for hidden/offstage legality
- a snapshot/live swap path to reduce relayout cost during animation

This architecture has accumulated multiple geometry and smoothness fixes, but
the underlying problem remains the same: we are manually animating a
user-resizable split pane while both the sidebar subtree and the content subtree
must keep legal GTK width floors throughout the transition.

That approach is also a poor foundation for adding a second sidebar on the
right.

## Goals

1. Support a permanent left workspace sidebar on desktop.
2. Add a right info/properties sidebar that can mimic GNOME Text Editor's info
   panel.
3. Allow both sidebars to be visible at the same time on wide windows.
4. Provide toolkit-owned, smooth show/hide behavior across desktop and narrow
   sizes.
5. Preserve simple toggle semantics for both panes.
6. Collapse gracefully on narrower widths without custom overlay or snapshot
   choreography.

## Non-Goals

- Preserving arbitrary drag-resizable sidebar widths.
- Keeping `GtkPaned` as the top-level left workspace/content container.
- Recreating IDE-style docking and panel rearrangement.

If LushText later needs fully user-dockable panes, that is a separate decision
and should be evaluated against `libpanel`.

## Recommended Architecture

### Core Layout

Use two nested `AdwOverlaySplitView`s:

```text
AdwApplicationWindow
└── GtkBox (vertical)
    ├── AdwHeaderBar
    ├── AdwTabBar
    ├── GtkOverlay
    │   └── AdwOverlaySplitView  [workspace_split_view]
    │       ├── sidebar  -> LushtextSidebarHost
    │       └── content  -> AdwOverlaySplitView [properties_split_view]
    │           ├── content -> MainContentHost
    │           └── sidebar -> LushtextPropertiesPanel
    └── LushtextStatusBar
```

### Why `AdwOverlaySplitView` for Both

`AdwOverlaySplitView` is a better fit than `AdwNavigationSplitView` for both
panes in this specific UI:

- the left workspace pane already has a desktop toggle and should continue to
  behave like a hideable utility sidebar, not like a navigation drill-down
  stack
- the right info pane is explicitly a utility pane
- both panes need a direct boolean toggle model via `show-sidebar`
- both panes can share the same breakpoint and visibility patterns

`AdwNavigationSplitView` was considered for the left pane, but it is less
aligned with the current desktop behavior because its collapsed mode is
navigation-driven (`show-content`) rather than utility-pane-driven
(`show-sidebar`).

## Pane Responsibilities

### Outer Split: `workspace_split_view`

Purpose:

- host the left workspace tree
- own the main left-pane toggle
- collapse the workspace sidebar into an overlay on narrow widths

Recommended properties:

- `sidebar-position = GTK_PACK_START`
- `min-sidebar-width = 180sp`
- `max-sidebar-width = 280sp`
- `sidebar-width-fraction ~= 0.20`
- `show-sidebar = true` by default on desktop

Child:

- `LushtextSidebarHost`
  - wraps the existing `LushtextSidebar`
  - owns sidebar-specific styling and optional future toolbar/header controls

### Inner Split: `properties_split_view`

Purpose:

- host the future right properties/info pane
- keep the main editor/search stack as the central content
- collapse the right pane into an overlay before the left pane does

Recommended properties:

- `sidebar-position = GTK_PACK_END`
- `min-sidebar-width = 260sp`
- `max-sidebar-width = 380sp`
- `sidebar-width-fraction ~= 0.28`
- `show-sidebar = false` by default

Child:

- `LushtextPropertiesPanel`
  - document metadata
  - formatting controls
  - file statistics
  - future outline/bookmarks/annotations hooks as appropriate

## Toggle Model

### Left Workspace Toggle

Keep the existing sidebar toggle affordance, but bind it to:

- `workspace_split_view:show-sidebar`

This preserves the current intent of the button while moving the behavior onto a
toolkit-owned split view.

### Right Properties Toggle

Add a new info/properties toggle button in the header bar, bound to:

- `properties_split_view:show-sidebar`

This is the natural place to mirror GNOME Text Editor's info toggle.

### Wide vs Narrow Semantics

On wide windows:

- `show-sidebar = true` means side-by-side pane
- `show-sidebar = false` hides the pane without custom paned animation

On collapsed windows:

- `show-sidebar = true` means overlay pane is visible
- `show-sidebar = false` means overlay pane is hidden

This gives both toggles one stable mental model across breakpoints.

## Breakpoints

Use two breakpoints, collapsing the right pane first and the left pane second.

Suggested starting points:

1. `max-width: 1100sp`
   - `properties_split_view.collapsed = true`
2. `max-width: 860sp`
   - `workspace_split_view.collapsed = true`

Resulting behavior:

- **Wide desktop**: left visible, center visible, right optional side-by-side
- **Medium desktop**: left visible, center visible, right overlays when opened
- **Narrow**: left overlays when opened; right overlays inside the central
  content flow

These numbers are starting points, not final UX law. They should be tuned with
real content and widget measurements.

## State and Settings

The current pixel-based `sidebar-position` setting is tied to draggable
`GtkPaned` behavior and should not remain the long-term source of truth.

### New Settings

Introduce separate left/right visibility and sizing policy keys:

- `workspace-sidebar-visible: bool`
- `workspace-sidebar-width-fraction: double`
- `properties-sidebar-visible: bool`
- `properties-sidebar-width-fraction: double`

Optional if needed:

- `properties-sidebar-tab: string`
- `workspace-sidebar-collapsed-override: bool`

### Migration

On first launch after migration:

- map the old `sidebar-visible` key to `workspace-sidebar-visible`
- derive an initial `workspace-sidebar-width-fraction` from the last saved pixel
  width and current window width, then clamp it into the new min/max range
- stop treating old pixel width as authoritative after migration

No right-pane migration is needed because the pane is new.

## Central Content Boundary

The central editor/search/preview stack should remain a single content host.

That host continues to own:

- `AdwTabView`
- `content_stack`
- search panel
- preview pane
- future split-editor work

The new split views should wrap this host rather than mixing sidebar state into
editor state.

## Suggested Migration Phases

### Phase 1: Structural Replacement

1. Replace `main_paned` with nested `AdwOverlaySplitView`s in `window.ui`.
2. Move the existing `LushtextSidebar` into `workspace_split_view.sidebar`.
3. Move the current content host into `properties_split_view.content`.
4. Stub `LushtextPropertiesPanel` with a placeholder widget.

### Phase 2: Actions and Settings

1. Rebind `win.toggle-sidebar` to `workspace_split_view:show-sidebar`.
2. Add `win.toggle-properties` for `properties_split_view:show-sidebar`.
3. Introduce the new visibility/fraction settings.
4. Add one-shot migration from old left sidebar settings.

### Phase 3: Breakpoint Behavior

1. Add breakpoints for inner then outer collapse.
2. Tune width fractions and min/max widths with real content.
3. Verify focus, keyboard shortcuts, and overlay dismissal rules.

### Phase 4: Properties Panel

1. Implement `LushtextPropertiesPanel`.
2. Port the planned metadata/info controls into it.
3. Align with GNOME Text Editor's information density and row patterns without
   cloning its exact UI blindly.

### Phase 5: Cleanup

1. Remove custom sidebar snapshot animation code.
2. Remove paned-specific width-floor persistence logic that only existed to
   support manual divider animation.
3. Retire stale tests tied to `GtkPaned` sidebar motion and replace them with
   split-view behavior tests.

## Testing Strategy

### Widget Tests

- left toggle shows/hides `workspace_split_view`
- right toggle shows/hides `properties_split_view`
- both panes can be visible on wide widths
- right pane collapses before left pane at breakpoints
- narrow mode overlays dismiss correctly
- settings restore visibility and width fraction

### Live GTK Verification

- no `Trying to measure ... needs at least ...` warnings during rapid toggles
- no first-toggle distortion after resize
- both sidebars remain smooth under repeated open/close cycles
- focus returns correctly after closing either overlay pane

## Risks

1. **Loss of arbitrary drag widths**
   - This is intentional, but it is still a behavior change.

2. **Migration complexity**
   - The window template, actions, settings, and tests all change together.

3. **Overlay interaction layering**
   - With both panes collapsed on narrow widths, dismissal and focus rules need
     to stay explicit and predictable.

4. **Future split-editor feature interaction**
   - The central content host must stay isolated enough that future editor split
     work does not leak sidebar assumptions back into the window shell.

## Alternatives Considered

### Keep `GtkPaned` for the Left Sidebar and Add a Right Split View

Rejected as the long-term plan.

It would reduce migration scope, but it keeps the most fragile part of the
current architecture in place: manual animation of the left workspace sidebar
across arbitrary widths.

### `AdwNavigationSplitView` for the Left Sidebar

Reasonable, but not the recommended first choice.

It is strongest when the collapsed behavior should become a navigation flow.
LushText's current left pane is better modeled as a hideable workspace utility
pane with a persistent desktop toggle, so `AdwOverlaySplitView` matches the
desired semantics more directly.

### `libpanel`

Deferred.

If LushText later needs IDE-style docking, movable panels, or more than two
sidebars/panes, `libpanel` becomes a serious candidate. It is heavier than what
is needed for the current "left workspace + right properties" goal.

## Supersedes

- `docs/next/adaptive-sidebar.md`

The earlier document assumed preserving the current `GtkPaned` model. That is
no longer the recommended direction because it does not solve the smoothness and
future-right-sidebar requirements together.

## References

- Libadwaita adaptive layouts:
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/adaptive-layouts.html
- `AdwOverlaySplitView`:
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.OverlaySplitView.html
- `AdwNavigationSplitView`:
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.NavigationSplitView.html
