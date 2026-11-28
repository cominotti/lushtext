# Delta Specification: gtk-lush-signals

## ADDED Requirements

### Requirement: Single replaceable signal slot avoids vector allocation
`gtk-lush-signals` SHALL provide a `SignalSlot` type owning at most one weak-source `SignalHandlerId` without heap vector allocation. Replacing or clearing the slot MUST disconnect any live prior handler once, and dropping the slot MUST disconnect any live handler safely even if the source was finalized.

#### Scenario: Replacing a handler disconnects the previous one
- **WHEN** `SignalSlot::replace` records a new handler while a previous handler on a live source is stored
- **THEN** the previous handler is disconnected immediately before storing the new registration
- **AND** `is_connected()` reports `true` while a registration is held

#### Scenario: Clearing or dropping an empty or dead-source slot is a no-op
- **WHEN** `SignalSlot::clear` or `Drop` runs on an empty slot or after the weak source finalized
- **THEN** no disconnect call or GLib warning occurs and `is_connected()` returns `false`

## MODIFIED Requirements

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
