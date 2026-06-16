## Context

Open popover rows are intentionally compact and source-compatible with GNOME Text Editor. Long file names and parent paths ellipsize inside the row so the popover can keep its fixed width, ten-row viewport, and no-horizontal-scrollbar contract.

The data needed for the hover text already exists. `RecentDocumentRow::from_entry()` stores the path opened by row activation, `OpenPopoverItem::from_row()` carries that path into the GTK list model, and the `SignalListItemFactory` bind callback already refreshes title, subtitle, age text, and accessible row label every time GTK binds an item to a reusable row widget.

## Goals / Non-Goals

**Goals:**

- Reveal the exact full absolute path that row activation will open.
- Keep GNOME-style row layout, row sizing, and text ellipsizing unchanged.
- Preserve the remove button's existing `Remove` tooltip and activation semantics.
- Prevent stale hover text when GTK recycles row widgets during long-list scrolling or filtering.
- Cover representative, awkward, dense, filtered, and action-control states with widget tests.

**Non-Goals:**

- No persistence format change for recent documents.
- No new canonicalization, existence check, or filesystem probe while binding rows.
- No new visible path column, subtitle expansion, horizontal scrolling, or row-size change.
- No change to file chooser, row activation, remove action, or recent-history ordering behavior.

## Decisions

1. Use `OpenPopoverItem::path()` as the tooltip source.

   Rationale: this is the same path passed to row activation and removal, so the hover text describes the file the row operates on. It also avoids confusing symlink or canonical identity differences.

   Alternative considered: use `canonical_path` or recompute a canonical path during binding. That would make hover text drift from the user-facing activation path and could add filesystem work to a GTK hot path.

2. Set the path tooltip during `SignalListItemFactory::connect_bind`.

   Rationale: GTK list rows are reusable. Updating the tooltip on every bind keeps hover text tied to the currently bound `OpenPopoverItem` after scrolling, filtering, or model replacement.

   Alternative considered: set the tooltip only during setup. Setup runs once per reusable widget subtree, before the specific recent item is known, so it cannot safely represent per-row data.

3. Apply the path tooltip to the row's non-action hover surface, while leaving the remove button tooltip as `Remove`.

   Rationale: users hovering the row should see the full path, while users hovering the close button should get the action description for that destructive control. This preserves current accessibility and GNOME-style control behavior.

   Alternative considered: assign the path tooltip to every descendant. That would overwrite the remove button's action-specific tooltip and make the close target less clear.

4. Keep display formatting simple: `item.path().display().to_string()`.

   Rationale: recent rows are local file-backed paths, and the existing row subtitle already uses display formatting for user-facing path text. No escaping, truncation, or async work is needed.

   Alternative considered: introduce a helper or service-level formatter. The behavior is narrow and directly tied to GTK tooltip text, so a new abstraction would add more structure than the change needs.

## Risks / Trade-offs

- Recycled row shows a stale path -> Mitigate by setting the tooltip in every bind and covering dense/filtering scenarios in widget tests.
- Remove button loses its `Remove` tooltip -> Mitigate with an explicit regression test that inspects the button tooltip after row binding.
- Tooltip source accidentally uses canonical identity rather than activation path -> Mitigate with a test that uses a row whose activation path is the asserted tooltip string.
- Very long path still cannot be fully inspected if the platform tooltip visually wraps or clips -> Mitigate by storing the complete unellipsized string in the GTK tooltip property; rendering limits are toolkit-controlled.

## Migration Plan

This is a runtime UI behavior change only. Existing recent-document persistence remains valid, and rollback is just removing the row tooltip binding and associated tests.

## Open Questions

None.
