# Accessibility Keyboard Coverage — Delta

## ADDED Requirements

### Requirement: Warning allowlist classification has a single source of truth

The accessibility smoke lane's warning allowlist classification SHALL be
defined exactly once in a shared module and imported by every scan and
summary path that classifies warning lines. The lane MUST NOT embed
duplicate copies of the classification predicate whose bodies can be edited
independently.

#### Scenario: Scan and summary paths classify identically

- **WHEN** the smoke lane classifies warning lines during the final warning
  scan and again while composing the summary artifact
- **THEN** both paths call the same shared classification predicate
- **AND** a warning line cannot be allowlisted by one path and unexpected by
  the other

#### Scenario: An allowlist change is single-site

- **WHEN** a maintainer adds, narrows, or removes an allowlist entry (for
  example a new compositor-shutdown noise pattern)
- **THEN** the change is made in one shared module
- **AND** no second embedded copy of the predicate exists to fall out of
  sync

#### Scenario: Consolidation preserves classification behavior

- **WHEN** the shared module replaces the previously duplicated predicates
- **THEN** ANSI style sequences are still stripped before classification
- **AND** every previously allowlisted line class remains allowlisted and
  every previously unexpected line class remains unexpected
