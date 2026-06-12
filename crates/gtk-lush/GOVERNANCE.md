# GTK Lush Governance

GTK Lush exists to extract LushText's hardened GTK4/Libadwaita patterns into
small, independently adoptable Rust crates. The program is governed so it stays
a leaf-crate family, not a framework.

## Constitution

Every GTK Lush crate, API, and follow-up change must pass this checklist:

- [ ] No ownership of GTK control flow. GTK owns the main loop, widget
      lifecycle, rendering, and scheduling; GTK Lush reacts to GTK.
- [ ] No view DSL. Blueprint, GtkBuilder XML, and ordinary gtk-rs code stay
      authoritative. Macros are allowed only when derive-style and additive.
- [ ] No state, message, or component system. The family must not introduce a
      model/update loop, actor tree, component hierarchy, or message bus.
- [ ] Leaf crates only. A GTK Lush crate must not depend on another GTK Lush
      crate as a runtime dependency.
- [ ] Adwaita remains authoritative for adaptive behavior. The family must not
      reimplement split views, breakpoints, sheets, or animations provided by
      Libadwaita.
- [ ] Pixels and contracts over claims. Each crate must ship the proof
      appropriate to its surface: docs, doctests, unit tests, widget tests for
      widget-facing behavior, and visual-geometry evidence for rendered
      invariants.

If an API cannot satisfy the afternoon-adoption test -- a stock gtk-rs
application can adopt exactly one crate in an afternoon without restructuring
anything else -- the API is redesigned or rejected.

## Exception Register

No constitution exceptions are approved.

Future exceptions must be added here before merge. Each entry must include:

- date and approver
- affected crate/API
- violated constitution principle
- invariant that makes the exception safe
- sunset condition or removal plan

## Treadmill SLAs

GTK Lush treats upstream GTK and Rust movement as part of maintenance, not as
optional cleanup.

- New gtk-rs major releases are supported within one GTK Lush family release
  cycle.
- The tested GNOME SDK floor may be raised at most once per calendar year.
- At publication time, the MSRV must be no newer than latest stable minus two.
- A blocked gtk-rs bump stops functional publishing rather than forking GTK or
  Libadwaita behavior.

## Publishing Gates

`0.0.0` reservations may exist only to protect names. A reservation package
must contain no public API, must point readers to `docs/next/gtk-lush.md`, and
must say plainly that it is not ready for functional use.

No crate may publish `0.1.0` or later until all gates below are true:

- LushText is one real consumer.
- At least one additional non-contrived application is a real consumer.
- A fresh timed afternoon-adoption test has passed for the crate.
- All friction from that adoption test is filed as issues or fixed.
- `cargo-semver-checks` and public-API tooling are green.
- The crate README, doctests, examples, changelog, and docs.rs metadata satisfy
  the engineering bar.
- The final constitution checklist for the release is recorded in this file.

Publishing requires explicit maintainer approval. Placeholder reservations and
functional releases must not be automated from CI without that approval.

## Maintenance Honesty

Each family crate must list a bus-factor plan before functional publication.
Until there is a broader maintainer group, the minimum plan is:

- keep the API surface small enough for a single maintainer to audit;
- prefer upstream documentation or bug reports when that would eliminate a
  carried workaround;
- document any release-blocking maintainer, gtk-rs, GNOME SDK, or MSRV risk in
  the release checklist.

Maintainer handoff must be deliberate before any functional publication. A
handoff entry in this file must name the outgoing and incoming maintainers,
list the crates and release authority being transferred, link to the final
review or issue, and record the date the incoming maintainer verified local
build, policy, MSRV, semver, public-API, and publication credentials. Until a
handoff is recorded, the previous maintainer remains accountable and functional
publishing stays stopped if that maintainer cannot review the release.

If a crate or the family can no longer meet the treadmill SLAs or maintainer
coverage commitments, functional publishing stops. The recovery choice must be
recorded before the next release:

- recovery plan with owner and deadline, or
- archive/deprecation decision with migration notes.

Silent rot is not allowed. An unmaintained crate must be archived deliberately.

## Repository Graduation

GTK Lush stays in this repository until the Phase 5 publishing gates pass. At
graduation, the family moves to a dedicated `gtk-lush` repository with history
preserved, and LushText consumes published versions. Path dependencies are
allowed only during the in-tree and graduation transition windows.

## Review Log

### 2026-06-10 — Foundation Audit (`establish-gtk-lush-program`)

Scope: initial `0.0.0` placeholder packages for `gtk-lush-signals` and
`gtk-lush-settle`, plus workspace, policy, documentation, and CI rails.

- [x] No ownership of GTK control flow. The crates expose no public API, and
      their standalone examples use a stock `gtk::Application`.
- [x] No view DSL. The foundation adds no macro, builder, template, or custom
      syntax surface.
- [x] No state, message, or component system. The placeholders contain only
      crate documentation.
- [x] Leaf crates only. `make check-gtk-lush-policy` rejects runtime
      dependencies on LushText crates or other GTK Lush crates.
- [x] Adwaita remains authoritative. No adaptive behavior is implemented or
      replaced in this foundation change.
- [x] Pixels and contracts over claims. The foundation proof is policy,
      packaging, doctests, standalone example compilation, MSRV verification,
      dependency policy, and the full fast LushText gate set; no runtime or
      visual-sensitive files changed, so widget and visual-geometry lanes were
      not required.

Exceptions: none.

Publication posture: only explicit maintainer approval may publish the `0.0.0`
reservations, and no functional `0.1.0` release may proceed until the
publishing gates above are satisfied and this log records the release audit.

### 2026-06-11 — Phase 2 Functional API Audit (`extract-gtk-lush-signals-and-settle`)

Scope: first functional in-tree `0.0.0` APIs for `gtk-lush-signals` and
`gtk-lush-settle`, plus LushText migration from fitting manual signal,
binding, and private settle-helper ownership.

- [x] No ownership of GTK control flow. The crates store registrations and
      schedule callbacks on GLib's existing main loop; GTK still owns widget
      lifecycle, rendering, and dispatch.
- [x] No view DSL. The APIs use ordinary gtk-rs `connect_*`,
      `bind_property`, and GLib timer calls; no macros or custom UI syntax were
      added.
- [x] No state, message, or component system. The crates expose RAII
      registration owners and generation counters only.
- [x] Leaf crates only. `gtk-lush-signals` and `gtk-lush-settle` have no
      runtime dependency on LushText or any other GTK Lush crate.
- [x] Adwaita remains authoritative. No adaptive behavior, split view,
      breakpoint, sheet, or animation API is reimplemented.
- [x] Pixels and contracts over claims. The phase proof includes crate unit
      tests, property tests, doctests, standalone example compilation, LushText
      migration tests, policy checks, widget verification, and visual proof
      where affected.

Exceptions: none.

Publication posture: these are functional in-tree `0.0.0` APIs for LushText
and future adoption testing only. They are not Phase 5 publication-ready, and
no `0.1.0` release may proceed until the publishing gates above are satisfied.

### 2026-06-11 — Phase 3 Runtime Geometry Audit (`extract-gtk-lush-runtime-geometry`)

Scope: functional in-tree `0.0.0` APIs for `gtk-lush-tasks`,
`gtk-lush-viewport`, and `gtk-lush-widgets`, plus LushText migration from the
app-local background dispatcher, viewport adjustment bookkeeping, shrinkable
content bin, and minimap render-hold mechanics.

- [x] No ownership of GTK control flow. The task crate uses GLib main-loop
      dispatch, viewport observation reacts to GTK adjustments, and widgets
      stay ordinary GTK objects.
- [x] No view DSL. Templates remain Blueprint/GtkBuilder XML, and widget
      adoption uses normal gtk-rs type registration.
- [x] No state, message, or component system. Freshness tokens, rest-state
      helpers, `ClipBin`, and `RenderHoldOverlay` do not introduce app state
      ownership or message routing.
- [x] Leaf crates only. The Phase 3 crates have no runtime dependency on
      LushText or any other GTK Lush crate.
- [x] Adwaita remains authoritative. Split views, breakpoints, sheets, and
      adaptive behavior stay in Libadwaita and LushText shell code.
- [x] Pixels and contracts over claims. The phase proof includes crate tests,
      doctests/examples, LushText compile checks, policy checks, widget proof,
      and visual-geometry proof for minimap/sidebar render invariants.

Exceptions: none.

Publication posture: these are functional in-tree `0.0.0` APIs for LushText
and future adoption testing only. They are not Phase 5 publication-ready, and
no `0.1.0` release may proceed until the publishing gates above are satisfied.
