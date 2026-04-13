# UI Driving Adapters

This subtree contains GTK4/Libadwaita driving adapters.

## Core Rules

- Keep widgets as adapters. `mod.rs` is the public facade; `imp.rs` holds template children, state, and signal glue.
- Keep signal closures thin and delegating. If a closure grows real decision logic, move it into a widget method, helper module, or service.
- Keep blocking I/O off the GTK main thread. Use `spawn_blocking_then` for filesystem work that can stall the UI.
- Build GTK collections and presentation models here, not in `services/`.
- Split large widget folders by workflow before adding traits or faux-manager types.

## Nested Guidance

- `window/`, `sidebar/`, `editor_page/`, and `search_panel/` have their own local `AGENTS.md` files. Read the nearest one before making local structural changes.
