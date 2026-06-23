# adw-sidebar-view-switcher-spike Specification

## Purpose

Define how LushText evaluates GNOME 50 `AdwSidebar` and
`AdwViewSwitcherSidebar` adoption opportunities before product-facing UI changes
are proposed.

## Requirements

### Requirement: Sidebar spike preserves existing workspace tree ownership
The system SHALL evaluate GNOME 50 sidebar widgets without replacing the primary workspace file tree or weakening its existing ownership contract.

#### Scenario: Primary workspace tree is evaluated
- **WHEN** the spike reviews the primary workspace sidebar
- **THEN** it MUST record that the file tree remains on `GtkListView`, `GtkTreeListModel`, and `GtkTreeExpander` unless a separate future proposal proves full parity for filesystem hierarchy, file operations, file peek, async scanning, deep-folder focus, watcher reconciliation, workspace scope, and constrained-width clipping

#### Scenario: Candidate uses tree-shaped or filesystem-mutating data
- **WHEN** a candidate surface requires arbitrary hierarchy expansion, row factories, inline rename, context menus, drag-and-drop reordering, filesystem mutation, or recursive loading
- **THEN** the spike MUST reject `AdwSidebar` and `AdwViewSwitcherSidebar` for that surface or split the work into a separate proposal with explicit parity requirements

### Requirement: Sidebar family candidates are classified by widget fit
The system SHALL classify each `AdwSidebar` or `AdwViewSwitcherSidebar` candidate by the data shape and navigation model the widget is designed to support.

#### Scenario: Dynamic shallow browse rows are reviewed
- **WHEN** a candidate surface presents dynamic, shallow, sectioned records whose selection previews or activates one record
- **THEN** the spike MUST evaluate `AdwSidebar` fit and record whether existing Notes or Local History adoption already covers the need

#### Scenario: Stable page navigation is reviewed
- **WHEN** a candidate surface presents a small stable set of page-like destinations
- **THEN** the spike MUST evaluate whether the surface can be modeled as an `AdwViewStack` before recommending `AdwViewSwitcherSidebar`

#### Scenario: Candidate lacks stable pages
- **WHEN** a candidate surface cannot name stable `AdwViewStack` pages and instead depends on changing records, files, or search results
- **THEN** the spike MUST reject `AdwViewSwitcherSidebar` for that surface and record the better-fitting widget family or defer the candidate

### Requirement: View-switcher spike covers state extremes
The system SHALL evaluate any `AdwViewSwitcherSidebar` candidate across visible state extremes before recommending implementation.

#### Scenario: No-context state is evaluated
- **WHEN** a candidate has no saved file, no workspace, no document metadata, or no records to show
- **THEN** the spike MUST verify that empty states remain readable, commands remain reachable, and no fake rows or unrelated context are required

#### Scenario: Representative populated state is evaluated
- **WHEN** a candidate has ordinary populated data
- **THEN** the spike MUST verify that page labels, icons, selection, activation, focus restoration, and accessible names describe the active workflow without duplicating existing controls

#### Scenario: Dense or awkward state is evaluated
- **WHEN** a candidate has many pages, long labels, warning states, missing metadata, or mixed available and unavailable pages
- **THEN** the spike MUST verify that item-region scrolling, truncation, disabled states, and warnings remain clear without unintended horizontal scrolling

#### Scenario: Constrained geometry is evaluated
- **WHEN** a candidate is shown in narrow width, short height, compact layout, large text, or reduced-motion environments
- **THEN** the spike MUST verify that header, back, close, primary actions, focus targets, and content remain visible or intentionally suppressed by the workflow contract

### Requirement: Spike output records an adoption decision
The system SHALL produce evidence and a recommendation for each sidebar/view-switcher candidate before implementation work begins.

#### Scenario: Candidate review completes
- **WHEN** the spike finishes reviewing a candidate surface
- **THEN** it MUST record the candidate, current implementation, widget fit, state-extreme findings, risks, verification evidence, and one recommendation: adopt in a separate proposal, defer, reject, or already covered

#### Scenario: Recommendation changes product behavior
- **WHEN** the recommendation would add a new surface, remove an existing surface, alter document-properties navigation, or change workspace/sidebar behavior
- **THEN** the spike MUST require a separate OpenSpec proposal before product code changes are implemented
