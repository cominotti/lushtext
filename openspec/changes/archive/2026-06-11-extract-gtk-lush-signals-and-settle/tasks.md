## 1. Orientation and Baseline Audits

- [x] 1.1 Read `proposal.md`, `design.md`, both capability specs, `docs/next/gtk-lush.md`, `crates/gtk-lush/GOVERNANCE.md`, and the archived Phase 0/1 artifacts before coding.
- [x] 1.2 Capture a current search inventory for manual signal ownership, binding ownership, private settle imports, timer-like generation counters, and retained explicit timing classes.
- [x] 1.3 Create or update an implementation audit note in this change for signal/binding migration candidates and retained explicit registration sites.
- [x] 1.4 Create or update an implementation audit note in this change for settle/timer migration candidates and retained explicit timing sites.
- [x] 1.5 Confirm the apply branch has no conflicting active OpenSpec work touching GTK Lush crates, `crate::ui::settle`, or the same rule sections.

## 2. `gtk-lush-signals` Public API

- [x] 2.1 Replace the placeholder crate docs with functional crate-level documentation that states the constitution, adoption test, and pre-Phase-5 publishing posture.
- [x] 2.2 Design and implement the core RAII signal registration owner with idempotent clear/drop behavior.
- [x] 2.3 Add support for weak or dead-source-tolerant registrations on long-lived/shared GObject sources such as settings and style-manager objects.
- [x] 2.4 Add grouped ownership or equivalent ergonomics so callers can clear one lifecycle family without disturbing unrelated registrations.
- [x] 2.5 Add RAII ownership for `glib::Binding` values with idempotent unbind-on-clear/drop behavior.
- [x] 2.6 Add explicit registration primitives for controller-like, row-owned, or transient registration lifetimes that fit the first functional API.
- [x] 2.7 Keep the API free of macros or trait extensions that shadow broad gtk-rs `connect_*` surfaces unless the design is explicitly documented and still additive.
- [x] 2.8 Ensure `gtk-lush-signals` has no runtime dependency on LushText crates or any other GTK Lush crate.
- [x] 2.9 Keep `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, SPDX headers, license metadata, and workspace lint compliance intact.

## 3. `gtk-lush-signals` Proof and Documentation

- [x] 3.1 Add unit tests for disconnect-on-clear, disconnect-on-drop, clear/drop idempotence, and no-post-drop callback behavior.
- [x] 3.2 Add tests for dead-source tolerance or weak-source skip behavior without GLib criticals.
- [x] 3.3 Add tests proving shared long-lived source registrations do not keep a consumer widget or object alive after owner teardown.
- [x] 3.4 Add tests for binding unbind behavior, including recycled-row or rebinding-style ownership.
- [x] 3.5 Add doctests for public signal and binding helper APIs where behavior is observable.
- [x] 3.6 Rewrite `crates/gtk-lush/signals/README.md` from the corresponding rule material, including examples, anti-framework limits, retained explicit classes, and pre-publication status.
- [x] 3.7 Update the `gtk-lush-signals` example so it demonstrates single-crate adoption of the functional API in a stock gtk-rs app.
- [x] 3.8 Update `crates/gtk-lush/signals/CHANGELOG.md` for the first functional in-tree API.

## 4. `gtk-lush-settle` Public API

- [x] 4.1 Replace the placeholder crate docs with functional crate-level documentation that states the constitution, adoption test, and pre-Phase-5 publishing posture.
- [x] 4.2 Split deterministic generation, staleness, and pending-state logic from GLib scheduling adapters.
- [x] 4.3 Design and implement the `Debounce`-class primitive for trailing latest-generation work with weak target cancellation.
- [x] 4.4 Provide generation advance/invalidate/current-token behavior needed by immediate-empty rebuilds and async freshness checks tied to the same debounce family.
- [x] 4.5 Design and implement the `SettleBurst`-class primitive with readiness-visible `pending()` state and same-dispatch repair completion semantics.
- [x] 4.6 Decide whether same-generation follow-up scheduling is required for migrated minimap or preview paths; if exposed, document it as settle-follow-up work, not a general scheduler.
- [x] 4.7 Design and implement the `SupersedingTimer`-class primitive for delayed latest-generation cleanup/reveal work.
- [x] 4.8 Ensure scheduling uses GLib/GTK main-loop mechanisms and does not introduce a custom runtime, executor, message loop, or component lifecycle.
- [x] 4.9 Ensure `gtk-lush-settle` has no runtime dependency on LushText crates or any other GTK Lush crate.
- [x] 4.10 Keep `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, SPDX headers, license metadata, and workspace lint compliance intact.

## 5. `gtk-lush-settle` Proof and Documentation

- [x] 5.1 Add unit tests for generation advancement, stale-token rejection, invalidation, wrapping behavior, and weak-target cancellation.
- [x] 5.2 Add property tests for pure debounce and settle-burst state transitions.
- [x] 5.3 Add tests proving stale settle handles cannot clear current pending state.
- [x] 5.4 Add tests proving pending remains true until current repair completion and clears only after the repair point.
- [x] 5.5 Add tests proving superseding one-shot re-arm and invalidation semantics.
- [x] 5.6 Add doctests for public debounce, settle-burst, and superseding-timer APIs where behavior is observable.
- [x] 5.7 Rewrite `crates/gtk-lush/settle/README.md` from the corresponding rule material, including choosing guidance, non-settle exceptions, examples, anti-framework limits, and pre-publication status.
- [x] 5.8 Update the `gtk-lush-settle` example so it demonstrates single-crate adoption of the functional API in a stock gtk-rs app.
- [x] 5.9 Update `crates/gtk-lush/settle/CHANGELOG.md` for the first functional in-tree API.

## 6. LushText Signal and Binding Migration

- [x] 6.1 Add workspace dependency wiring so LushText consumes `gtk-lush-signals` through the established in-tree path setup.
- [x] 6.2 Migrate low-risk local widget signal ownership sites and remove the replaced manual handler fields or row-data disconnect code.
- [x] 6.3 Migrate editor preference/settings/style-manager signal ownership, preserving tab open/close handler cleanup.
- [x] 6.4 Migrate editor buffer-swapping signal ownership for modified, changed, minimap, focus-mode, and local-history handlers.
- [x] 6.5 Migrate search bar, command palette, sidebar, workspace-section, notes browser, preferences, and window signal ownership where each site fits the crate contract.
- [x] 6.6 Migrate stored or repeated `glib::Binding` ownership patterns that fit widget, row, or workflow lifetimes.
- [x] 6.7 After each migration batch, run focused compile/tests or widget tests that exercise the migrated teardown path.
- [x] 6.8 Update the signal/binding audit so every remaining manual handler, binding, or transient registration site is classified with a reason.
- [x] 6.9 Remove obsolete app-local helper code or duplicated lifecycle wrappers once their last migrated consumer is gone.

## 7. LushText Settle Migration

- [x] 7.1 Add workspace dependency wiring so LushText consumes `gtk-lush-settle` through the established in-tree path setup.
- [x] 7.2 Migrate low-risk `SupersedingTimer` sites such as status pulse cleanup and focus-mode affordance hide while preserving durations.
- [x] 7.3 Migrate search-like `Debounce` sites for command palette, content search, glob filtering, notes browser, bookmark dialog, and index flushes.
- [x] 7.4 Migrate persistence debounce sites while preserving domain ordered-save generations, dirty/inflight state, and latest-state-wins semantics.
- [x] 7.5 Migrate refresh, monitor, preview-render, and focus-indexing debounce sites while preserving stale async result rejection.
- [x] 7.6 Migrate preview layout settle and minimap refresh/reflow/reveal settle paths with readiness-visible pending behavior intact.
- [x] 7.7 Keep recurring pollers, heartbeats, chunked yields, idle repair loops, async worker freshness tokens, and domain generations explicit unless the audit proves a fit and tests cover it.
- [x] 7.8 After each migration batch, run focused tests or widget tests that exercise empty state, representative state, rapid changes, and teardown behavior for the migrated surface.
- [x] 7.9 Update the settle/timer audit so every remaining timer-like or generation-counter site is classified with a reason.
- [x] 7.10 Delete `crates/lushtext-core/src/ui/settle.rs` or reduce it only to documented temporary compatibility glue with a removal task completed in this change.

## 8. UX, Readiness, and Visual Proof

- [x] 8.1 Add, update, or verify widget tests for migrated search and picker debounces covering empty input, representative input, rapid input, many/awkward results, and constrained geometry where relevant.
- [x] 8.2 Add, update, or verify widget tests for migrated persistence debounces proving older scheduled or in-flight saves cannot overwrite newer state.
- [x] 8.3 Add or update widget tests for migrated signal ownership proving closed or recycled widgets do not receive callbacks.
- [x] 8.4 Verify GTK/GLib warning gates after timers fire against destroyed, hidden, recycled, or superseded widgets.
- [x] 8.5 Verify automation readiness blockers remain equivalent for any migrated minimap, preview, or layout settle paths.
- [x] 8.6 Run `make check-automation-docs` and `make automation-client-self-test` if readiness fields, automation snapshots, or documented automation behavior changed.
- [x] 8.7 Run visual-geometry proof for minimap, preview, or other rendered-geometry-sensitive migrations, including pixel-anchor and animation-stream scenarios where affected.

## 9. Guidance, Governance, and Roadmap Updates

- [x] 9.1 Update `.agents/rules/widget-wiring.md` to require `gtk-lush-signals` and `gtk-lush-settle` for fitting new work and to document retained explicit exception classes.
- [x] 9.2 Update `.agents/rules/rust.md` where handler lifetime, GTK main-thread scheduling, or generation-token guidance now belongs to the crate docs.
- [x] 9.3 Update `docs/next/gtk-lush.md` so Phase 2 is marked complete or current, later phases remain reserved, and no Phase 5 publishing readiness is implied.
- [x] 9.4 Update root or nested `AGENTS.md` files if crate layout, module ownership, or rule index content materially changes.
- [x] 9.5 Update `crates/gtk-lush/GOVERNANCE.md` with a Phase 2 constitution checklist review entry and any approved exceptions; keep the exception register empty if none are approved.
- [x] 9.6 Run `make check-agent-docs` after guidance and AGENTS/rule edits.

## 10. Final Verification and Close-Out

- [x] 10.1 Run `cargo fmt --all -- --check`.
- [x] 10.2 Run family crate tests, doctests, and standalone example checks for `gtk-lush-signals` and `gtk-lush-settle`.
- [x] 10.3 Run `make check-gtk-lush-policy`.
- [x] 10.4 Run dependency/policy checks affected by workspace dependency changes, including cargo-deny and cargo-hakari checks required by the repo rules.
- [x] 10.5 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 10.6 Run the full non-widget test gate required by the repo for a phase boundary.
- [x] 10.7 Run the full headless widget suite.
- [x] 10.8 Run visual-geometry smoke/proof lanes required by the files changed in this phase.
- [x] 10.9 Run `openspec validate extract-gtk-lush-signals-and-settle --strict`.
- [x] 10.10 Run `openspec validate --changes --strict`, `openspec validate --specs --strict`, and `openspec validate --all --strict`.
- [x] 10.11 Run `git diff --check`.
- [x] 10.12 Record final verification results and any intentionally deferred non-settle or non-signal ownership classes in the change artifacts before archive.
