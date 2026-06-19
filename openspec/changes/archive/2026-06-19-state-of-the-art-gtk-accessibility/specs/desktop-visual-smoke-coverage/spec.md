## ADDED Requirements

### Requirement: Visual smoke SHALL cover accessibility-relevant desktop variants
The desktop visual smoke lane SHALL include reviewable coverage for visual accessibility variants where host support exists, including focus indication, high contrast, large text, reduced motion, color-not-only state communication, and readability under transparency.

#### Scenario: Focus indication is captured
- **WHEN** visual smoke captures keyboard navigation through shell controls, editor, rows, dialogs, popovers, bottom sheets, context menus, and search surfaces
- **THEN** the screenshots show the focused target with a visible focus indication
- **AND** artifacts identify the expected focused surface through automation or AT-SPI state before accepting the screenshot

#### Scenario: High contrast and dark variants are captured
- **WHEN** the host supports high-contrast or dark style capture
- **THEN** visual smoke captures representative shell, editor, search, dialog, and dense-list states under those variants
- **AND** warnings, errors, selections, disabled controls, destructive actions, and active focus remain distinguishable without relying on color alone

#### Scenario: Large text and constrained geometry are captured
- **WHEN** visual smoke runs with large text or a documented text-scale variant
- **THEN** primary actions, close/back controls, headers, status controls, and item-region scrolling remain visible in representative and constrained layouts
- **AND** clipping, overlap, unintended horizontal scrollbars, or hidden primary controls fail the scenario

### Requirement: Visual smoke SHALL prove reduced-motion and animation accessibility where supported
Visual smoke SHALL document and verify how motion-sensitive transitions behave when reduced-motion settings are supported by GTK, Libadwaita, or the host session.

#### Scenario: Reduced-motion environment is recorded
- **WHEN** visual smoke runs a reduced-motion variant
- **THEN** it records the host setting, GTK/Libadwaita behavior, renderer, scale, and theme metadata
- **AND** unsupported or ineffective host settings are reported clearly rather than counted as verified coverage

#### Scenario: Transitions do not hide controls during motion
- **WHEN** visual smoke captures sidebar, properties, search, command palette, preview, focus mode, or dialog transitions
- **THEN** persistent controls and focus targets remain visible or intentionally suppressed according to the workflow contract
- **AND** intermediate animation frames do not expose overlapping controls, clipped focus rings, or stale transient surfaces as passing states

#### Scenario: Motion alternatives remain keyboard accessible
- **WHEN** an animation-sensitive workflow is exercised with reduced motion or normal motion
- **THEN** keyboard activation, cancellation, and focus restoration follow the same semantic path
- **AND** screenshots and state artifacts distinguish intended visual differences from accessibility regressions

### Requirement: Visual smoke SHALL verify color-not-only communication
Visual smoke SHALL include scenarios that show LushText communicating important state through text, iconography, role/state metadata, shape, or position in addition to color.

#### Scenario: Alerts and destructive states have non-color cues
- **WHEN** warning, error, durability, recovery, destructive, or save/close states are visible
- **THEN** visual smoke shows non-color cues such as text, iconography, layout, role/state-backed controls, or explicit action labels
- **AND** high-contrast and dark variants preserve those cues

#### Scenario: Editing and navigation states have non-color cues
- **WHEN** modified tabs, search matches, selected rows, bookmarks, local-history restore state, disabled actions, or file-health states are visible
- **THEN** the state is distinguishable through more than hue alone
- **AND** accessibility metadata for the same state is verified in a companion widget, automation, or AT-SPI assertion

#### Scenario: Transparency does not undermine readability
- **WHEN** editor or Markdown preview background opacity is below full opacity
- **THEN** visual smoke verifies that text, focus, selection, search matches, inline alerts, and preview content remain readable in representative light and dark contexts
- **AND** unrelated opaque chrome remains unaffected by document-surface transparency

### Requirement: Visual accessibility artifacts SHALL be reviewable and bounded
Visual accessibility smoke artifacts SHALL preserve enough context to diagnose accessibility visual regressions without large golden-image sets or unbounded user content.

#### Scenario: Variant manifest records environment and assertions
- **WHEN** a visual accessibility scenario finishes
- **THEN** its manifest records style variant, text scale, motion setting, renderer, window size, fixture kind, expected focused surface, protected controls, and warning-scan status
- **AND** screenshots and logs are referenced by path rather than embedded in terminal output

#### Scenario: Unsupported variants skip explicitly
- **WHEN** high contrast, large text, reduced motion, screenshot capture, compositor, AT-SPI, or renderer support is unavailable
- **THEN** the visual smoke lane records a distinct unsupported status and reason
- **AND** unsupported variants do not count as verified visual accessibility coverage
