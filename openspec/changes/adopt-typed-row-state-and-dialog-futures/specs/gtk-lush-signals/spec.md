# Spec Delta

## ADDED Requirements

### Requirement: LushText stores no state as untyped GObject data
LushText production code SHALL NOT read or attach Rust values on GObjects through `set_data`, `steal_data`, `data` or their `qdata` forms. A binding or signal owner tied to a recycled list row SHALL live in typed state with a known owner, released when the row is unbound, and per-row markers SHALL be typed state compared by object identity. A policy audit SHALL fail on any such call in `crates/lushtext-core/src`.

#### Scenario: Recycled search row drops its count binding
- **WHEN** a search-result file row is unbound and its list item is rebound to another file
- **THEN** the previous item's count binding is unbound
- **AND** the badge shows the new item's count, not the previous one

#### Scenario: Recycled sidebar row drops its accessibility hook
- **WHEN** a sidebar directory row is unbound and recycled for another directory
- **THEN** the previous row's expanded-state accessibility handler is disconnected
- **AND** the recycled row announces only the new directory's expanded state

#### Scenario: A recreated tree row gets its expansion watch
- **WHEN** GTK creates a new `GtkTreeListRow` for an item whose previous row was instrumented
- **THEN** the new row receives the expansion watch exactly once
- **AND** a collapsed row is not retained by the marker

#### Scenario: Two live rows for one item each get one watch
- **WHEN** two `GtkTreeListRow`s for the same item are alive at once and both are seen by the watch
- **THEN** each has exactly one expansion handler

#### Scenario: A drag collapse cannot leave a suppression armed
- **WHEN** a reorder drag collapses a folder row
- **THEN** no suppression remains after the collapse returns, whether or not a handler consumed it
- **AND** the row's next user expansion restarts its watch
- **AND** child rows torn down by the collapse are not suppressed

#### Scenario: A row's bindings may be a binding group
- **WHEN** a row widget owns its item bindings through one binding group retargeted at bind and unbind
- **THEN** a rebound row shows only the new item's values
- **AND** the retained-site audit lists the row as a GTK-native binding owner

#### Scenario: Untyped object data fails the audit
- **WHEN** code under `crates/lushtext-core/src` calls `set_data`, `steal_data`, `data`, `set_qdata`, `steal_qdata` or `qdata`
- **THEN** the policy audit fails and names the file and line

#### Scenario: Tests and probes stay outside the audit
- **WHEN** a widget test under `crates/lushtext/tests/` or a GTK axiom probe uses object data to drive the toolkit
- **THEN** the audit does not report it
