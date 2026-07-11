## 1. Baseline and Behavior Inventories

- [ ] 1.1 Rebase this change after the other seven portfolio changes and record a clean `make pre-commit` plus relevant widget/accessibility/visual/automation baseline.
- [ ] 1.2 Inventory window note actions, methods, callbacks, persistence/generation guards, dialogs, menus, browser states, palette routes, migrations, and automation anchors.
- [ ] 1.3 Inventory adaptive-shell inputs/outputs, constants, breakpoint parse/install conditions, settings reads, allocation mutations, focus paths, and existing pure tests.
- [ ] 1.4 Inventory workspace-section factory setup/bind/unbind object data, signal/binding bags, DnD/rename handoffs, context actions/keyboard routes, accessibility projection, and disposal cleanup.

## 2. Pure Adaptive-Shell Extraction

- [ ] 2.1 Create `ui/window/adaptive_shell.rs` with plain input/output values, width/fraction/breakpoint math, compact-surface arbitration, and current unit tests.
- [ ] 2.2 Leave template children, GSettings reads/writes, Libadwaita breakpoint objects, focus, signal setup, allocation application, and disposal in `window/imp.rs`.
- [ ] 2.3 Verify every inventoried representative and boundary input returns behavior-equivalent decisions before removing old pure helpers from `imp.rs`.
- [ ] 2.4 Run focused adaptive widget and visual-geometry tests for wide, compact, constrained-height, both-requested, animation, and restored-intent states.

## 3. Workspace-Section Wiring Extraction

- [ ] 3.1 Create `row_accessibility.rs` and move row labels/descriptions/position/expanded/disabled projection plus expanded-hook install/clear symmetry.
- [ ] 3.2 Create `context_menus.rs` and move file/header menu construction, action rows, pointer/keyboard target resolution, popup lifecycle, and public selection/header entry points.
- [ ] 3.3 Create `row_factory.rs` and move setup/bind/unbind projection, binding/signal bags, object-data cleanup, DnD handoff, focus controls, and inline-rename trigger.
- [ ] 3.4 Keep subclass state, template children, `constructed`, `dispose`, and short setup calls in `workspace_section/imp.rs` with the narrowest practical module visibility.
- [ ] 3.5 Run recycling/lifecycle tests plus zero/one/many/overlap/reorder/focus/rename/context/accessibility/constrained sidebar coverage after each extraction.

## 4. Window Notes Workflow Extraction

- [ ] 4.1 Convert `window/notes.rs` to a private `window/notes/` module with `mod.rs` retaining shared facade types, callback routing, common coordination, and menu availability.
- [ ] 4.2 Move bookmark toggle/edit/navigation/persistence/browser/excerpt/activation and bookmark-specific migration behavior to `notes/bookmarks.rs`.
- [ ] 4.3 Move document/folder note targeting, editor dialogs, preview/save/discard lifecycle, sidecar resolution, and note-specific migration behavior to `notes/editors.rs`.
- [ ] 4.4 Move Browse Notes search/category/sidebar/async preview state plus command-palette note-source refresh and target activation to `notes/browser.rs`.
- [ ] 4.5 Resolve privacy with private or `pub(super)` items instead of broad `pub(crate)` exposure, managers, traits, new widgets, or service-layer GTK types.
- [ ] 4.6 Run focused tests after each workflow move for persistence faults, generation races, empty/one/many states, awkward paths, no active editor, focus, accessibility, and constrained geometry.

## 5. Documentation and Full Behavior-Neutral Proof

- [ ] 5.1 Update root/crate README module maps and nearest window/sidebar/root `AGENTS.md` ownership guidance for the final file structure.
- [ ] 5.2 Run formatting, Rust comments/architecture review, compile, unit/integration/property tests, widget lifecycle/recycling tests, and strict rustdoc/lint gates.
- [ ] 5.3 Run `make check-agent-docs`, `make check-automation-docs`, `make automation-client-self-test`, accessibility smoke, visual smoke, and visual-geometry smoke; fix every issue found without accepting contract drift.
- [ ] 5.4 Run `make check`, `make lint-advisory`, and `make pre-commit`, inspect runtime logs for GTK/GDK/Libadwaita/GIO/accessibility warnings, and fix all pre-existing or introduced blockers in scope.
- [ ] 5.5 Compare the final action/D-Bus/snapshot/readiness inventory with baseline and treat any difference as a regression unless separately proposed.
- [ ] 5.6 Run the learning workflow, remove stale ownership guidance, and record only durable architectural lessons.
