## ADDED Requirements

### Requirement: Live runner starts its session through the helper
The `cargo-gtk-proof` live runner SHALL start its session bus, compositor, PipeWire and WirePlumber only through the `gtk-lush-proof-harness` session library, in the `screencast` profile. It SHALL report a `SESSION-HYGIENE:` verdict as a failed case and SHALL NOT count any invariant from that case as verified.

#### Scenario: A live case runs
- **WHEN** `cargo gtk-proof run` executes a visual-geometry case
- **THEN** the case's runtime report names the helper and its profile instead of `dbus-run-session`, and the case's artifacts include the helper's hygiene report

#### Scenario: A dirty session fails the case
- **WHEN** the helper reports a violation for a case's session
- **THEN** the case's status is failed with `failure_status: session-hygiene`, and the run's summary counts it
