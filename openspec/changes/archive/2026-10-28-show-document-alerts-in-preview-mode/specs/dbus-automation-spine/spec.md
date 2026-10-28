## ADDED Requirements

### Requirement: Automation snapshots report the preview document alert surface
The automation surface snapshot SHALL include `surfaces.preview_document_alert_visible`, a boolean that is true only while the Markdown preview's document-alert surface is rendered visible. The field MUST be derived from the rendered widget state, not from preview-mode state alone. It MUST be projected from the notification workflow's evidence surface, not read from the widget by the automation adapter. It MUST NOT carry alert title or body text. The field MUST be documented in `docs/automation.md` and `docs/automation-reference.md` and checked by `make check-automation-docs`.

#### Scenario: Preview-only mode with an alert reports the surface visible
- **WHEN** preview-only mode is active
- **AND** the selected document owns an inline alert
- **AND** a snapshot is requested after `accessibility-settled`
- **THEN** `surfaces.preview_mode` is true
- **AND** `surfaces.preview_document_alert_visible` is true

#### Scenario: Preview-only mode without an alert reports the surface hidden
- **WHEN** preview-only mode is active
- **AND** the selected document owns no inline alert
- **THEN** `surfaces.preview_document_alert_visible` is false

#### Scenario: Side-by-side preview never reports the surface visible
- **WHEN** side-by-side preview is active
- **AND** the selected document owns an inline alert
- **THEN** `surfaces.preview_pane_visible` is true
- **AND** `surfaces.preview_document_alert_visible` is false

#### Scenario: The field is documented
- **WHEN** `make check-automation-docs` runs
- **THEN** `snapshot-field-preview-document-alert-visible` is present in `docs/automation-reference.md` with type `bool`
