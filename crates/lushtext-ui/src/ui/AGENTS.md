# UI Driving Adapters

This subtree contains GTK4/Libadwaita driving adapters.

## Core Rules

- Keep widgets as adapters. `mod.rs` is the public facade; `imp.rs` holds template children, state, and signal glue.
- Keep signal closures thin and delegating. If a closure grows real decision logic, move it into a widget method, helper module, or service.
- Keep blocking I/O off the GTK main thread. Use `gtk_lush_tasks::spawn_blocking_then` for filesystem work that can stall the UI.
- Route destruction-only `Send` payloads through `plain_disposal`; its non-blocking two-worker lane admits at most eight reserved drop slots and 128 MiB of ordinary retained weight, releases capacity on worker completion, and allows one overweight job only when otherwise empty. Startup recovery and Notes source construction use the separate bounded 72 MiB progress lane so long-lived ordinary owners cannot starve them; replacements retain only the matching lane's credited headroom. Startup must demote eager bodies until its complete measured preload graph fits the reservation, and Browse Notes must reuse one active browser while bounding live-editor request metadata before admission. Document-sized results must reserve before they cross onto GTK and retain that reservation until accepted transfer or final worker destruction; capacity pressure retains only one latest compact request and one retry source. GTK objects never enter either lane, and GTK callbacks must never block on disposal admission.
- Build GTK collections and presentation models here, not in `services/`.
- Split large widget folders by workflow before adding traits or faux-manager types.
- Keep read-only automation D-Bus collection and readiness waits in
  `automation.rs`. They may observe UI state on the GTK main context, but must
  not perform blocking I/O, index work, or private widget mutation. Mutating
  automation setup belongs on normal app/window actions.
- UI templates are authored in `resources/ui/*.blp`; generated
  `resources/ui/*.ui` files stay committed for the GResource runtime contract.
  Edit `.blp`, run `make blueprint-generate`, then run `make check-blueprint`.
  Use `make lint-blueprint` for curated Blueprint lint triage that keeps
  promoted diagnostics clean and bounds accepted warnings, and use
  `scripts/compare-blueprint-visuals.sh --baseline-ref <ref>` when a
  geometry-sensitive template edit needs before/after visual proof.

## Nested Guidance

- `window/`, `sidebar/`, `editor_page/`, and `search_panel/` have their own local `AGENTS.md` files. Read the nearest one before making local structural changes.
