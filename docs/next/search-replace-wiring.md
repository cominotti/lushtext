# Search & Replace Wiring

## Status: Next priority

## Description
The search bar UI widget exists with find/replace fields and buttons, but it is not yet
wired to GtkSourceView's search infrastructure.

## Implementation Plan
1. In `LushtextEditorPage`, create `GtkSourceSearchSettings` and `GtkSourceSearchContext`
2. Bind `search_entry.text` → `SearchSettings::search_text`
3. Wire `next_button` → `SearchContext::forward()`, `prev_button` → `SearchContext::backward()`
4. Wire `replace_button` → `SearchContext::replace()`, `replace_all_button` → `SearchContext::replace_all()`
5. Update `match_label` from `SearchContext::occurrences_count` property
6. Wire `close_button` → hide search revealer
7. Wire `Escape` key in search entry → hide search revealer
8. Wire `Ctrl+G` / `Ctrl+Shift+G` → next/previous match
