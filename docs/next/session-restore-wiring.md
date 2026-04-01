# Session Restore Wiring

## Status: Next priority

## Description
The session service (save/load/filter) is implemented and tested, but not yet wired
to the GTK application lifecycle.

## Implementation Plan
1. In `LushtextWindow::constructed()` or `activate()`: call `session_service::load()`
   and re-open tabs from the saved session
2. In `LushtextWindow::close_request()`: collect all open tab states (path, cursor
   position, scroll position) and call `session_service::save()`
3. Handle edge cases: files deleted since last session (already handled by
   `filter_existing_tabs()`), empty sessions, corrupted session files
4. Restore scroll position and cursor position after file load
