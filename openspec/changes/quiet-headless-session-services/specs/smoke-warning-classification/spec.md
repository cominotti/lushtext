## ADDED Requirements

### Requirement: System-bus warnings of headless sessions are one shared family
Mutter's warnings about missing system services in a headless session SHALL be one shared family in the shared classifier module, matched by exact text. The family covers the private-system-bus `ServiceUnknown` lines and the no-system-bus "Could not connect" lines. Widget, axiom and smoke runners SHALL read the family from the module rather than keeping copies of those lines.

#### Scenario: The widget runner classifies a private-bus warning
- **WHEN** a widget session's log contains `libmutter-WARNING **: … There is no colord server available`
- **THEN** `run-widget-tests.sh` treats it as benign through the family it read from the shared module
- **AND** a near-miss line with different text still fails the scan

#### Scenario: A copy reappears in a runner
- **WHEN** a runner adds its own pattern for a line the family already covers
- **THEN** the existing divergence check flags the duplicate
