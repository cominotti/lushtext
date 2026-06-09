# UI Driving Adapters

This subtree contains GTK4/Libadwaita driving adapters.

## Core Rules

- Keep widgets as adapters. `mod.rs` is the public facade; `imp.rs` holds template children, state, and signal glue.
- Keep signal closures thin and delegating. If a closure grows real decision logic, move it into a widget method, helper module, or service.
- Keep blocking I/O off the GTK main thread. Use `spawn_blocking_then` for filesystem work that can stall the UI.
- Build GTK collections and presentation models here, not in `services/`.
- Split large widget folders by workflow before adding traits or faux-manager types.
- UI templates are authored in `resources/ui/*.blp`; generated
  `resources/ui/*.ui` files stay committed for the GResource runtime contract.
  Edit `.blp`, run `make blueprint-generate`, then run `make check-blueprint`.
  Use `make lint-blueprint` for curated Blueprint lint triage that keeps
  promoted diagnostics clean and bounds accepted warnings, and use
  `scripts/compare-blueprint-visuals.sh --baseline-ref <ref>` when a
  geometry-sensitive template edit needs before/after visual proof.

## Nested Guidance

- `window/`, `sidebar/`, `editor_page/`, and `search_panel/` have their own local `AGENTS.md` files. Read the nearest one before making local structural changes.
