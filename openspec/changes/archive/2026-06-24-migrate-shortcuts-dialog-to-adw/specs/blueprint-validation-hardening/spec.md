## MODIFIED Requirements

### Requirement: Blueprint compile validation SHALL classify warnings
The Blueprint compile gate SHALL keep generated `.ui` drift and template-contract
checks blocking, while allowing only documented known compiler warnings.

#### Scenario: Deprecated GtkShortcuts warnings fail the gate
- **WHEN** the compile gate processes any Blueprint template
- **THEN** warnings for deprecated `GtkShortcuts*` widgets are not accepted as
  known-good output
- **AND** `resources/ui/shortcuts.blp` uses the maintained Libadwaita shortcuts
  dialog widgets instead of the deprecated GTK shortcuts widget family

#### Scenario: Unknown compile warnings fail the gate
- **WHEN** `blueprint-compiler compile` emits a warning outside the documented
  known-warning policy
- **THEN** `make check-blueprint` fails
- **AND** the failure output identifies the file and warning text that must be
  fixed or classified

#### Scenario: Compiler version and template coverage are reported
- **WHEN** `make check-blueprint` runs
- **THEN** the output includes the `blueprint-compiler` version used for
  validation
- **AND** the output identifies the templates covered by the compile, drift, and
  contract checks
