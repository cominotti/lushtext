# Spec Delta

## ADDED Requirements

### Requirement: GTK-thread weak captures use one checked form
LushText UI code SHALL capture GObjects weakly in GTK-thread closures through `glib::clone!`. A closure that returns a value MUST state its upgrade-failure value explicitly, equal to the value its manual form returned. A policy audit SHALL hold `.upgrade()` calls on a bare local name in `crates/lushtext-core/src/ui` to recorded per-file counts, each exception with its reason, so a file above its count fails and a file below it must lower it; upgrades of fields are not counted.

#### Scenario: Manual weak capture above the ceiling fails
- **WHEN** a change adds a manual weak capture (a local `downgrade` moved into a closure and upgraded there) in a file of `crates/lushtext-core/src/ui` already at its recorded count
- **THEN** the audit fails and names the file and line

#### Scenario: A weak reference in a field does not count
- **WHEN** a change adds a struct field holding a `WeakRef` and upgrades it through `self`
- **THEN** the audit's count does not change

#### Scenario: A recorded exception admits a collection upgrade
- **WHEN** a change adds an upgrade of a weak reference taken from a collection and raises its file's exception with a reason
- **THEN** the audit passes
- **AND** the same change without the recorded exception fails

#### Scenario: Lowering the count lowers the record
- **WHEN** a change removes manual sites from a file
- **THEN** the audit fails until that file's recorded count is lowered to the new count

#### Scenario: A value-returning closure keeps its fallback
- **WHEN** a key-press handler's weak target is gone
- **THEN** the handler returns the same `glib::Propagation` value it returned before the rewrite

#### Scenario: Lints still see inside the macro
- **WHEN** a `clone!` body contains a call Clippy denies, such as `.unwrap()`
- **THEN** `make check-clippy` reports it
