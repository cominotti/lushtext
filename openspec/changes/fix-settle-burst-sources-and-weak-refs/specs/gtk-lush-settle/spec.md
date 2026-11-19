# Spec Delta

## ADDED Requirements

### Requirement: Settle burst keeps one pending source
A `SettleBurst`-class primitive SHALL keep at most one pending GLib source, and one weak reference to its target, however many times it is scheduled before the quiet window ends. Scheduling again MUST remove the superseded source, and clearing MUST remove the pending one, without changing the burst's pending state or which handle may clear it.

#### Scenario: A burst of schedules keeps one source
- **WHEN** a settle burst is scheduled 70,000 times in one dispatch
- **THEN** every superseded callback is dropped at once and only the latest stays pending
- **AND** GLib reports no weak-reference limit warning

#### Scenario: Pending state is unchanged by the removal
- **WHEN** a burst is rescheduled before its deadline
- **THEN** `pending()` stays true until the latest handle completes, as before

#### Scenario: Clear removes the pending source
- **WHEN** a pending burst is cleared
- **THEN** its callback is dropped without running and `pending()` is false
