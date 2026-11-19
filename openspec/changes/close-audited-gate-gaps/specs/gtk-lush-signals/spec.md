# Spec Delta

## ADDED Requirements

### Requirement: Shared-settings subscriptions in LushText are owned
Every LushText subscription to the shared application settings instance from a window, widget or other owner SHALL disconnect when that owner is disposed or finalized, through a registration owner or an owner-weak helper. A subscription that lives for the whole process MUST be allowlisted with its reason and expected count, and the settings gate MUST fail on a discarded handler id.

#### Scenario: Closing a window leaves no visibility handler behind
- **WHEN** a window is opened and closed and finalized
- **THEN** the number of `changed::workspace-show-hidden-files` and `changed::workspace-excluded-names` handlers on the shared settings equals the number before it opened

#### Scenario: The sidebar and window subscribe the same way
- **WHEN** the window and the sidebar subscribe to workspace-visibility changes
- **THEN** both pass their owner to the subscription helper
- **AND** neither keeps handler ids it must remember to disconnect

#### Scenario: Discarded handler id fails the settings gate
- **WHEN** code outside the owner helpers calls a two-argument settings `connect_changed(Some(…))` whose statement neither binds the id with `let` nor passes or returns it
- **THEN** `make check-settings-writes` fails and names the file and line
- **AND** a call split over lines after `let id =`, or one on a widget such as an entry or a buffer, does not fail

#### Scenario: Process-lifetime subscription is allowlisted
- **WHEN** `ui/theme.rs` installs its display-wide font and transparency subscriptions once at startup
- **THEN** the gate accepts exactly its expected number of such sites through an allowlist entry that names the process lifetime as the reason
- **AND** one more discarded subscription in that file fails the gate
