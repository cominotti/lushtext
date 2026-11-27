## MODIFIED Requirements

### Requirement: LushText consumes the extracted harness without command drift
LushText's widget test target SHALL become a consumer of
`gtk-lush-proof-harness` while preserving existing Makefile entry points,
default monitor behavior, child-test isolation, retry behavior, warning
filtering, and custom app setup.

#### Scenario: Existing widget commands still work
- **WHEN** a developer runs `make test-widget` or `make test-widget-headless`
- **THEN** the command runs the LushText widget suite through the extracted
  harness
- **AND** existing filters, list mode, retries, headless monitor defaults, and
  warning-scan behavior remain documented and functional

#### Scenario: LushText app setup stays local
- **WHEN** the widget suite constructs a LushText application or window
- **THEN** LushText-specific GResource registration, GSettings isolation,
  test-data directory setup, filesystem fixtures, and widget registry
  generation remain in LushText adapter code or explicit app callbacks
- **AND** `gtk-lush-proof-harness` does not import `lushtext-ui`, `lushtext-services`, or `lushtext-model`
