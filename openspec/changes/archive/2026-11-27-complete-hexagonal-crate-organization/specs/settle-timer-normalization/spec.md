## MODIFIED Requirements

### Requirement: Timer-like call sites are audited before conversion
The implementation SHALL audit GLib one-shot/recurring timers, idle deferrals, timeout futures, generation counters, and `SourceId` cancellations under `crates/lushtext-ui/src/ui` and related service helpers before converting, classifying each by lifecycle role.

#### Scenario: Debounce candidate is identified
- **WHEN** a GTK main-loop timer coalesces repeated user or model changes by
  capturing a generation and no-oping stale firings
- **THEN** the audit classifies it as debounce or superseding one-shot
- **AND** records the expected helper treatment

#### Scenario: Polling candidate is not forced into settle
- **WHEN** a recurring timer owns a lifecycle poll, heartbeat, autosave tick, or
  readiness loop
- **THEN** the audit classifies it as heartbeat/polling
- **AND** the implementation leaves it outside the private settle helper unless
  a later explicit design changes that lifecycle contract

### Requirement: Private helper prototypes future settle primitives
LushText SHALL introduce a private in-tree helper shape for Phase 0 timer
normalization. The helper MUST prototype the future `gtk-lush-settle`
contracts for debounce, delayed settle bursts, and superseding one-shot timers
without exposing a public GTK Lush crate API, adding a family crate dependency,
or replacing GTK's main-loop ownership.

#### Scenario: Helper stays application-internal
- **WHEN** the helper is added
- **THEN** it is private to the LushText application or `lushtext-ui`
- **AND** no new public `gtk-lush-settle` crate API is created or documented as
  available to external consumers

#### Scenario: Pure decision logic is testable
- **WHEN** the helper implements generation advancement, staleness decisions, or
  pending-state transitions
- **THEN** that deterministic logic is covered by unit or property tests without
  requiring a live GTK main loop
