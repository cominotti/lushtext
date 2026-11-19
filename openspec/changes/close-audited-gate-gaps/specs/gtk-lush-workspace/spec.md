# Spec Delta

## ADDED Requirements

### Requirement: Family crates stay out of workspace-hack's dependents
The GTK Lush family crates and the adoption lab SHALL count for cargo-hakari's feature unification but SHALL NOT depend on `workspace-hack`, so a stock consumer of one family crate never inherits LushText's dependency graph. The exclusion MUST be recorded in the hakari configuration, and the family policy check MUST fail when that list differs from the family's package list.

#### Scenario: Stock fixture keeps its graph
- **WHEN** `make gtk-lush-stock-fixtures` runs its fixtures with `--locked`
- **THEN** the fixtures resolve without any `workspace-hack` package

#### Scenario: A new family crate must be excluded
- **WHEN** a crate is added under `crates/gtk-lush/` without an entry in the hakari exclusion list
- **THEN** `make check-gtk-lush-policy` fails and names it

#### Scenario: Exclusion leaves the generated crate unchanged
- **WHEN** `cargo hakari generate --diff` runs with the recorded exclusions
- **THEN** it reports no difference
