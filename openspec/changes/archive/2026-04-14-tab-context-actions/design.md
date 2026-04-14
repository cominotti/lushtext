## Context

LushText already renders its document strip with `AdwTabBar` and `AdwTabView` in `resources/ui/window.ui`, and the window shell already owns the surrounding workflows that tab-management changes need to respect:

- `ui/window/actions.rs` registers window actions and their enabled state.
- `ui/window/dialogs.rs` already provides both single-tab and multi-tab save confirmation flows.
- `ui/window/documents.rs` and `ui/window/imp.rs` own close cleanup, selection updates, duplicate-path tracking, and session-save triggers.
- `ui/window/session_persistence.rs` serializes tab order by walking the current `AdwTabView`, but `SessionTab` currently stores no pin state.

The libadwaita bindings already expose the main primitives needed for this feature: tab-view menu models, setup hooks for per-page context menus, pinning APIs, bulk close helpers, and directional reordering. The design work is therefore mostly about fitting those primitives into LushText's existing save-close, draft-cleanup, and session-restore contracts without leaking more mixed responsibility into `imp.rs`.

## Goals / Non-Goals

**Goals:**

- Add a native tab-strip context menu for per-tab management actions.
- Support `Pin` or `Unpin`, `Close All Tabs to the Right`, `Close Other Tabs`, `Move Left`, and `Move Right`.
- Keep pinned tabs grouped ahead of normal tabs and restore that arrangement across restarts.
- Reuse the existing save-changes dialog flow for bulk close so modified tabs remain safe.
- Keep implementation ownership clear by placing the new tab-management workflow in a focused window module.

**Non-Goals:**

- Adding new keyboard shortcuts for these actions in the first iteration.
- Adding extra tab-strip actions such as `Close Tabs to the Left`, `Move to Start`, `Move to End`, or tab groups.
- Changing drag-and-drop tab reordering or multi-window tab transfer behavior.
- Redesigning the whole tab bar or adding a separate overview UI.

## Decisions

### 1. Use `AdwTabView`'s native menu hook and keep tab-management logic in a dedicated window workflow module

The implementation should use `AdwTabView::set_menu_model()` plus the setup-menu callback instead of inventing a custom right-click gesture or manual popover anchored to tab widgets. The menu is already conceptually owned by the tab view, and the setup callback provides the exact `TabPage` the user invoked so actions can target the clicked tab rather than whichever tab happens to be selected.

To keep the window shell readable, the page-target resolution, menu-state refresh, pin helpers, movement helpers, and bulk-close orchestration should live in a new sibling workflow module such as `ui/window/tabs.rs`, with only the minimum state handles added to `imp.rs`.

Alternatives considered:

- Custom gesture plus `GtkPopoverMenu`: rejected because it would duplicate toolkit behavior and create more fragile tab-hit testing.
- Adding the actions directly to `documents.rs` or `imp.rs`: rejected because this is a new shell workflow with its own state and safety rules.

### 2. Treat pinning as first-class persisted layout state

`SessionTab` should gain a `pinned: bool` field with a serde default so existing `session.json` files remain valid. Session collection should record both the current visual order and each page's pinned state. Session restore should then recreate pinned pages in their saved relative order ahead of restored unpinned pages, so a restart returns the same arrangement the user last set up.

Pinning should also trigger debounced session saves when it changes, instead of waiting for a later tab selection or app shutdown. The same applies to drag or action-driven reordering through `page-reordered`, because tab arrangement is part of the session contract once we expose explicit move actions.

Alternatives considered:

- Keep pinning as ephemeral UI state only: rejected because pinned placement would feel broken after restart.
- Store pinned tabs in a separate list outside `SessionTab`: rejected because the current session model already uses ordered tab snapshots, and a second structure would add avoidable restore complexity.

### 3. Define pinned tabs as protected anchors for bulk-close actions

Pinned tabs should stay grouped at the leading side of the tab strip and be excluded from `Close Other Tabs` and `Close All Tabs to the Right` target sets. This makes the feature match the practical purpose of pinning: tabs the user wants to keep around while they churn through a temporary working set of normal tabs.

Because of that rule, bulk close should not rely solely on `AdwTabView::close_other_pages()` or `close_pages_after()`. Instead, the implementation should compute the exact target pages from the clicked `TabPage`, current page order, and pin state, filter out pinned pages, and close only the resulting set.

Alternatives considered:

- Let bulk close affect pinned tabs too: rejected because it weakens the value of pinning and makes destructive actions harder to trust.
- Delegate all targeting to libadwaita bulk-close helpers: rejected because the desired pinned-tab protection rule is app-level behavior and should remain explicit.

### 4. Reuse the existing multi-editor save confirmation before bulk close

The current `show_save_changes_dialog()` and `save_editors_for_close()` flows already handle multiple modified editors safely, including untitled-tab blocking and draft cleanup rules. Bulk-close actions should therefore gather the target pages first, derive the subset of modified editors from that list, run the existing save dialog once if needed, and only then close the final page set in a stable order.

Closing from right to left remains the safest order for the actual `close_page()` calls because it avoids page-position drift while detach callbacks update `open_paths`, monitors, and session state. The important point is that the close-confirmation step happens before any destructive page removal completes, so the bulk action does not fan out into overlapping per-tab dialogs.

Alternatives considered:

- Call `close_page()` in a naive loop and let `connect_close_page` prompt per tab: rejected because it risks a noisy or overlapping confirmation experience.
- Create a second custom bulk-save dialog path just for tab actions: rejected because the existing dialog already matches the needed contract.

### 5. Keep left or right movement segment-aware and boundary-limited

`Move Left` and `Move Right` should use the existing `AdwTabView` reorder primitives, but only when the target page can move within its current segment. A pinned tab can move only among pinned tabs; an unpinned tab can move only among unpinned tabs and must never cross ahead of the pinned segment boundary. The menu should disable movement actions when no eligible move exists.

This keeps pinning and ordering rules consistent without introducing extra "repair" logic after every move.

Alternatives considered:

- Allow move actions to cross the pinned boundary and then infer pin changes: rejected because it conflates reordering with pinning.
- Always show actions as enabled and silently no-op at boundaries: rejected because disabled state communicates the contract more clearly.

## Risks / Trade-offs

- [Pinned tabs being excluded from bulk close may differ from some editors] -> Mitigation: make the rule explicit in the spec and keep the context-menu labels and behavior consistent so users learn that pinning protects those tabs.
- [The context menu targets the clicked tab, which may differ from the selected editor] -> Mitigation: keep the target-page plumbing explicit and avoid silently retargeting to the selected page when the menu opens.
- [Session model changes can regress older restore behavior if not made backward-compatible] -> Mitigation: add `#[serde(default)]` for the new `pinned` field and extend unit coverage for old-session deserialization.
- [New menu-state and reorder-save signals could add shell complexity] -> Mitigation: isolate them in a dedicated `tabs.rs` workflow module and keep `imp.rs` limited to small state holders and signal hookup.

## Migration Plan

No user-facing migration flow is required. Existing session files should remain readable by defaulting missing `pinned` values to `false`. Once the feature ships, newly saved sessions will preserve both pin state and tab order. Rollback remains low risk because the change is confined to window-shell tab management and session metadata; the underlying documents, drafts, and workspaces remain unchanged.

## Open Questions

None blocking. The implementation can decide whether pinned tabs need an additional explicit pin icon or whether Adwaita's pinned placement plus the `Pin` or `Unpin` context label is sufficient, as long as pinned state remains clear and testable.
