# lushtext-core

This crate owns the application's real behavior: domain types, services, and GTK driving adapters.

## Boundaries

- Keep dependency direction `ui/ -> services/ -> model/`.
- Keep `model/` framework-free: no GTK, GLib, gio, or service/UI imports.
- Keep `services/` GTK-free. Background work should use `gtk_lush_tasks`
  from callers that own the relevant GTK-thread state and freshness policy,
  rather than recreating GLib task glue in application services.
- Keep GTK collections and widget-facing models in `ui/`; services should return plain Rust data.
- Keep automation contract data split by layer: `model/` owns serializable
  value objects, `services::action_catalog` owns the GTK-free action inventory
  and audits, and `ui/automation.rs` is the only app-owned D-Bus adapter.

## Structure

- Treat `ui/` as driving adapters. Split big widget folders by workflow before inventing new abstraction layers.
- Keep responsive main-window calculations in `ui/window/policy.rs` as plain
  inputs and decisions; `ui/window/imp.rs` retains GTK objects, settings,
  allocation application, focus, signals, and disposal.
- Keep the private `ui/window/notes/` facade responsible for shared note
  coordination, with bookmark, editor, and browser workflows in their named
  siblings. Keep recycled workspace-row factory, accessibility, and context
  menu wiring in their named `ui/sidebar/workspace_section/` modules.
- Treat `services/` as application logic and driven adapters. Prefer free functions unless a trait is clearly justified.
- Treat `model/` as the home for invariants, value objects, and repeated field bundles.
- Treat `services::filesystem` as the only production filesystem adapter. Prefer `metadata::exists` or `metadata::path_status` for cheap status probes and keep `metadata::file_facts` for workflows that need canonical identity, byte size, or mtime.
- Keep old metadata-format structs, parsers, and converter chains sealed under
  `services::format_upgrade::legacy`. Ordinary runtime readers, domain models,
  and UI adapters must continue to know only the latest supported app-owned
  format; upgrade preview/apply values returned to UI must stay plain Rust data.

## Editing Rules

- When a subtree gains stable local contracts, prefer a nested `AGENTS.md` over stuffing more volatile detail into the repo root file.
- Keep nested files local and non-duplicative. The root `AGENTS.md` should stay the canonical repo-wide contract.
- Update `AGENTS.md` and `README.md` module-layout sections whenever modules are added, removed, or materially reorganized.
- When an exported action, D-Bus automation member, snapshot field, readiness
  predicate/blocker, or scenario-helper flag changes, update `docs/automation.md` plus
  `docs/automation-reference.md` and run `make check-automation-docs`.
