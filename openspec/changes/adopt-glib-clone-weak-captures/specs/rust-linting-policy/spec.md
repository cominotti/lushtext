# Spec Delta

## ADDED Requirements

### Requirement: GTK-thread weak captures use one checked form
LushText UI code SHALL capture GObjects weakly in GTK-thread closures through `glib::clone!`. A closure that returns a value MUST state its upgrade-failure value explicitly, equal to the value its manual form returned. A policy audit SHALL hold the number of `.upgrade()` calls on a bare local name in `crates/lushtext-core/src/ui` at or below a recorded ceiling that only decreases; upgrades of weak references stored in fields are not counted.

#### Scenario: Manual weak capture above the ceiling fails
- **WHEN** a change adds a manual weak capture (a local `downgrade` moved into a closure and upgraded there) in `crates/lushtext-core/src/ui` while the count is at its ceiling
- **THEN** the audit fails and names the file and line

#### Scenario: A weak reference in a field does not count
- **WHEN** a change adds a struct field holding a `WeakRef` and upgrades it through `self`
- **THEN** the audit's count does not change

#### Scenario: Lowering the count lowers the ceiling
- **WHEN** a change removes manual sites
- **THEN** the audit fails until the recorded ceiling is lowered to the new count

#### Scenario: A value-returning closure keeps its fallback
- **WHEN** a key-press handler's weak target is gone
- **THEN** the handler returns the same `glib::Propagation` value it returned before the rewrite

#### Scenario: Lints still see inside the macro
- **WHEN** a `clone!` body contains a call Clippy denies, such as `.unwrap()`
- **THEN** `make check-clippy` reports it
