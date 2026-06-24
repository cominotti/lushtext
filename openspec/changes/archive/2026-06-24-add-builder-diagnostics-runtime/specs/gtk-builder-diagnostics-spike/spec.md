## ADDED Requirements

### Requirement: Builder diagnostics spike hands off to automated diagnostics
The system SHALL treat the completed builder diagnostics spike as evidence for an automated diagnostics lane rather than as the final operating model.

#### Scenario: Future work consults the spike
- **WHEN** future planning or implementation work consults `gtk-builder-diagnostics-spike`
- **THEN** it MUST use the `automated-builder-diagnostics` capability for reusable runtime, CI, coverage, classification, and enforcement requirements
- **AND** it MUST treat the manual `GTK_DEBUG=builder,builder-objects` recipe as historical or focused-debugging evidence, not the complete diagnostics workflow

#### Scenario: Spike evidence remains useful
- **WHEN** a builder diagnostics implementation needs prior evidence
- **THEN** it MAY reference the spike's command lines, limitations, and findings
- **AND** it MUST still satisfy the automated diagnostics capability before claiming local or CI coverage
