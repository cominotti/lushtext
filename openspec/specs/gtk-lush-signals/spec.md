# gtk-lush-signals Specification

## Purpose
Define the `gtk-lush-signals` leaf crate contract for RAII signal, binding,
and registration ownership helpers, plus LushText migration and proof gates, so
GTK lifetime bookkeeping remains explicit, testable, and stock
gtk-rs-compatible.

## Requirements

### Requirement: Leaf RAII registration crate
`gtk-lush-signals` SHALL provide RAII lifetime helpers for stock gtk-rs
applications while remaining an independently adoptable GTK Lush leaf crate.
The crate MUST NOT depend on LushText crates, MUST NOT depend on another
GTK Lush family crate at runtime, MUST NOT wrap GTK widget lifecycle or signal
delivery, and MUST NOT replace ordinary gtk-rs `connect_*`, property binding,
controller, or subclassing APIs. Consumers MUST be able to store the helper
types in ordinary structs or GObject implementation structs without adopting a
component system, message loop, custom view syntax, or application framework.

#### Scenario: Standalone application adopts only signals
- **WHEN** `cargo test -p gtk-lush-signals --examples` builds the crate's
  standalone example
- **THEN** the example uses stock gtk-rs widgets plus `gtk-lush-signals`
- **AND** no other GTK Lush crate or LushText crate is required

#### Scenario: Runtime family dependency is rejected
- **WHEN** `gtk-lush-signals` declares another `gtk-lush-*` crate as a
  non-dev dependency
- **THEN** the family policy check fails until the dependency is removed

#### Scenario: Ordinary gtk-rs signal APIs remain visible
- **WHEN** a consumer connects a signal through the crate's helper
- **THEN** the underlying connection still uses gtk-rs signal APIs and returns
  or records the normal signal registration identity
- **AND** the crate does not introduce a replacement signal DSL

### Requirement: Signal registrations disconnect exactly once
`gtk-lush-signals` SHALL provide `SignalBag` for grouped weak-source GObject signal registrations without requiring `Clone` on the tracked source, and disconnect every live handler exactly once on `clear` or `Drop`. Cleanup ordering and disposed-source preconditions MUST be documented, and containers MUST implement `Debug`.

#### Scenario: Drop disconnects handlers
- **WHEN** a handler is recorded in the owner and the owner is dropped while the emitting object is still alive
- **THEN** the handler is disconnected exactly once
- **AND** a later signal emission from that object does not run the callback

#### Scenario: Clear is idempotent
- **WHEN** an owner with recorded handlers is cleared twice and then dropped
- **THEN** every recorded live handler is disconnected once
- **AND** no second disconnect attempt, panic, or GLib critical is emitted

#### Scenario: Group clear leaves other groups active
- **WHEN** a widget stores editor-buffer handlers and settings handlers in separate registration owners
- **THEN** clearing the editor-buffer owner disconnects only the buffer handlers
- **AND** the settings handlers remain connected until their own owner is cleared or dropped

#### Scenario: Re-entrant clear or track during disconnect does not panic
- **WHEN** disconnecting a handler triggers a closure destructor that calls `clear()` or `track()` on the same `SignalBag`
- **THEN** no `RefCell` borrow panic occurs and the new registration remains intact after the outer clear returns

### Requirement: Shared and long-lived sources use weak ownership
The crate SHALL support weak or dead-source-tolerant ownership for shared and
long-lived signal sources.

Signal registrations on objects that can outlive the consumer widget, such as
application-global `gio::Settings`, `libadwaita::StyleManager`, or shared
buffers, SHALL be recorded so the registration owner does not keep the
consumer widget alive after teardown. The crate MUST support weak source
tracking or equivalent dead-source tolerance for registrations whose source may
be finalized before the owner drops.

#### Scenario: Settings handler does not leak widget
- **WHEN** a widget-owned registration owner records a handler connected to a
  long-lived settings object
- **AND** the widget is destroyed
- **THEN** the registration is disconnected
- **AND** the handler closure can no longer keep the widget alive through a
  strong reference cycle

#### Scenario: Finalized source is tolerated
- **WHEN** a recorded signal source has already been finalized before the
  owner is cleared
- **THEN** clearing or dropping the owner skips that dead registration safely
- **AND** no panic or GLib warning is emitted

#### Scenario: Closure capture guidance is documented
- **WHEN** a consumer reads the crate README or public item docs
- **THEN** the docs explain that signal closures must still avoid strong
  reference cycles, and demonstrate weak captures for GTK objects that refer
  to each other

### Requirement: Property bindings unbind through RAII ownership
The crate SHALL provide RAII ownership for `glib::Binding` values and common
property-binding lifetimes. Recorded bindings MUST be unbound when their owner
is explicitly cleared or dropped, and unbinding MUST be idempotent. The
binding owner MUST support one-way and two-way binding shapes used by GTK,
GSettings, and widget-state projections without hiding the underlying gtk-rs
binding builder semantics.

#### Scenario: Binding owner unbinds on clear
- **WHEN** a property binding is recorded in a binding owner
- **AND** the owner is cleared
- **THEN** the binding is unbound
- **AND** subsequent source property changes no longer propagate through that
  binding

#### Scenario: Recycled row clears previous binding
- **WHEN** a list row or factory-created item is rebound from one model object
  to another
- **THEN** clearing the previous binding owner unbinds the old model
  projection before the new binding is recorded
- **AND** changes to the old model no longer update the recycled row

#### Scenario: Two-way setting binding is owned explicitly
- **WHEN** a two-way settings-to-widget binding is recorded in the owner
- **THEN** dropping or clearing the owner unbinds that binding without
  requiring the caller to retain the raw `glib::Binding` separately

### Requirement: Controller and transient registrations have explicit ownership
The crate SHALL support registration lifetimes that are signal-like but not
plain object fields, including event-controller handlers, row-owned transient
registrations, and other GTK registrations that must be removed or
disconnected during recycle, unbind, dispose, or workflow teardown. The first
functional API MAY use explicit record/clear primitives rather than custom
helpers for every GTK type, but it MUST make the ownership boundary visible to
callers.

#### Scenario: Row-owned expansion handler is removed on unbind
- **WHEN** a row-owned handler is recorded while binding a virtualized list row
- **AND** GTK later unbinds or recycles that row
- **THEN** clearing the row's registration owner disconnects the old handler
- **AND** the recycled row does not react to the previous row's object

#### Scenario: Event-controller callback follows widget lifetime
- **WHEN** an event-controller callback is recorded in a widget-owned
  registration owner
- **THEN** clearing the owner prevents later controller events from invoking
  the callback
- **AND** dropping the owner after the controller is gone is safe

#### Scenario: Unsupported registration class remains explicit
- **WHEN** a GTK registration cannot be safely represented by the crate's
  first functional ownership helpers
- **THEN** the migration audit records it as an explicit retained site
- **AND** the rules do not claim that the crate owns that class yet

### Requirement: Public API documentation and tests prove lifetime behavior
Every public item in `gtk-lush-signals` SHALL keep `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, and tests proving `clear`, `Drop`, dead-source tolerance, `BindingBag::drop`, `RegistrationBag::drop`, `SignalSlot`, and re-entrant `clear`/`track` using stock GLib/GTK objects without LushText.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public signal, binding, slot, or registration helper is added without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Lifetime tests run without LushText
- **WHEN** `cargo test -p gtk-lush-signals` runs
- **THEN** signal, slot, binding, and registration lifetime tests execute using only stock gtk-rs and glib objects
- **AND** no LushText application crate is linked

#### Scenario: README teaches the owned bug class
- **WHEN** the README is rendered
- **THEN** it explains the GTK lifetime bug classes the crate prevents, shows adoption in a small stock gtk-rs example, and states the anti-framework constraints

### Requirement: LushText manual handler bookkeeping migrates to the crate
LushText SHALL migrate fitting manual `RefCell<Option<glib::SignalHandlerId>>` slots onto `gtk_lush_signals::SignalSlot` alongside existing `SignalBag` migrations. Replaced manual fields and disconnect boilerplate MUST be deleted while preserving observable workflow behavior.

#### Scenario: Editor preference handlers migrate
- **WHEN** editor preference and style handlers are migrated from manual handler-id fields to crate-owned registrations
- **THEN** opening and closing editor tabs does not accumulate handlers on shared settings or style-manager objects
- **AND** all editor preference behavior remains unchanged

#### Scenario: Buffer-swapping handlers migrate safely
- **WHEN** an editor page replaces or closes the current source buffer
- **THEN** crate-owned buffer handlers are disconnected from the old buffer before new handlers are recorded or the page is destroyed
- **AND** edits to the old buffer do not update the closed or replaced editor

#### Scenario: Window, sidebar, and row handlers migrate in batches
- **WHEN** fitting single-slot `SignalHandlerId` fields in `crates/lushtext-ui/src/` migrate to `SignalSlot`
- **THEN** at least five manual single-slot sites delete their raw `Option<SignalHandlerId>` bookkeeping
- **AND** focused tests or widget tests covering teardown and interaction behavior pass

### Requirement: LushText binding ownership migrates where it is lifecycle state
LushText SHALL migrate stored `glib::Binding` vectors, list-item binding data,
or repeated binding ownership patterns onto `gtk-lush-signals` when the
binding lifetime belongs to a widget, row, or workflow owner. Bindings that
remain explicit MUST be listed in the retained-site audit with the reason they
do not fit the first functional crate surface.

#### Scenario: Virtualized list bindings unbind on recycle
- **WHEN** a search, sidebar, notes, or command-palette list item is unbound
  or recycled
- **THEN** any migrated binding owner unbinds the old model projection
- **AND** the recycled widget does not display stale model state from the
  previous item

#### Scenario: Preference bindings remain behaviorally equivalent
- **WHEN** preference or settings bindings migrate to crate ownership
- **THEN** initial sync, two-way update behavior, and sensitivity projections
  remain equivalent to the previous `gio::Settings::bind` or
  `glib::Binding` usage

### Requirement: Retained explicit signal and binding sites are audited
The implementation SHALL produce an audit of signal, binding, and
registration-like sites that remain explicit after the migration. Each retained
site MUST be classified as unsupported registration shape, GTK-owned
virtualized-row lifecycle, one-shot local closure that does not need retained
ownership, pending future GTK Lush phase, or intentionally out of scope.

#### Scenario: Manual handler remains with a reason
- **WHEN** a `SignalHandlerId` field or row-data handler remains after the
  migration
- **THEN** the audit records the file, lifecycle owner, and reason it remains
  explicit
- **AND** the rule rewrite does not present it as a missed conversion

#### Scenario: No unexplained manual ownership remains
- **WHEN** the final audit searches LushText UI code for manual signal or
  binding ownership patterns
- **THEN** every remaining result is either migrated, outside the crate's
  contract, or recorded as an explicit exception

### Requirement: Rule and roadmap guidance follows proven migration
After LushText migration and proof, project guidance SHALL point at
`gtk-lush-signals` as the required mechanism for fitting signal and binding
lifetime ownership. `.agents/rules/rust.md`, `.agents/rules/widget-wiring.md`,
the crate README, and `docs/next/gtk-lush.md` MUST be updated in the same
change to describe the proven API, retained exception classes, and Phase 2
completion status without claiming Phase 5 publication readiness.

#### Scenario: Rules point at crate docs
- **WHEN** the migration completes
- **THEN** the relevant rules no longer teach new hand-rolled
  `RefCell<Option<SignalHandlerId>>` ownership for fitting sites
- **AND** they link or refer to the crate documentation for the reusable
  pattern

#### Scenario: Vision document advances the phase
- **WHEN** this change completes
- **THEN** `docs/next/gtk-lush.md` records that Phase 2 extracted the signals
  API
- **AND** the later phase roadmap remains consistent with the canonical
  governance spec

### Requirement: Signals migration preserves LushText proof gates
The signal and binding migration SHALL preserve LushText's full existing proof
surface. The phase MUST pass the family crate tests, doctests, standalone
examples, policy checks, relevant focused tests, the full non-widget gate set,
the widget suite, and the GTK/GLib warning gate. If a migrated signal or
binding affects rendered geometry or transient UI lifecycle, the relevant
visual or automation proof MUST run before archive.

#### Scenario: Widget teardown remains warning-clean
- **WHEN** the widget suite opens, interacts with, and tears down migrated
  windows, editors, sidebars, lists, and dialogs
- **THEN** no new unexpected GTK, GLib, or GObject warnings are emitted

#### Scenario: Behavior remains stable after tab churn
- **WHEN** many tabs are opened, closed, and reopened after the migration
- **THEN** preference changes, dark-mode updates, buffer edits, minimap
  updates, focus-mode handlers, and local-history handlers affect only live
  editors
- **AND** closed editors do not receive callbacks

#### Scenario: State extremes remain covered
- **WHEN** migrated signal or binding ownership affects a collection surface
  such as command results, notes, search results, workspaces, files, or tabs
- **THEN** tests cover no required context, representative populated data,
  many or awkward items, and constrained geometry when those states are
  relevant to the surface

### Requirement: Single replaceable signal slot avoids vector allocation
`gtk-lush-signals` SHALL provide a `SignalSlot` type owning at most one weak-source `SignalHandlerId` without heap vector allocation. Replacing or clearing the slot MUST disconnect any live prior handler once, and dropping the slot MUST disconnect any live handler safely even if the source was finalized.

#### Scenario: Replacing a handler disconnects the previous one
- **WHEN** `SignalSlot::replace` records a new handler while a previous handler on a live source is stored
- **THEN** the previous handler is disconnected immediately before storing the new registration
- **AND** `is_connected()` reports `true` while a registration is held

#### Scenario: Clearing or dropping an empty or dead-source slot is a no-op
- **WHEN** `SignalSlot::clear` or `Drop` runs on an empty slot or after the weak source finalized
- **THEN** no disconnect call or GLib warning occurs and `is_connected()` returns `false`
