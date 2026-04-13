# lushtext-core

This crate owns the application's real behavior: domain types, services, and GTK driving adapters.

## Boundaries

- Keep dependency direction `ui/ -> services/ -> model/`.
- Keep `model/` framework-free: no GTK, GLib, gio, or service/UI imports.
- Keep `services/` GTK-free except for explicit infrastructure glue such as `async_task.rs`.
- Keep GTK collections and widget-facing models in `ui/`; services should return plain Rust data.

## Structure

- Treat `ui/` as driving adapters. Split big widget folders by workflow before inventing new abstraction layers.
- Treat `services/` as application logic and driven adapters. Prefer free functions unless a trait is clearly justified.
- Treat `model/` as the home for invariants, value objects, and repeated field bundles.

## Editing Rules

- When a subtree gains stable local contracts, prefer a nested `AGENTS.md` over stuffing more volatile detail into the repo root file.
- Keep nested files local and non-duplicative. The root `AGENTS.md` should stay the canonical repo-wide contract.
- Update `AGENTS.md` and `README.md` module-layout sections whenever modules are added, removed, or materially reorganized.
