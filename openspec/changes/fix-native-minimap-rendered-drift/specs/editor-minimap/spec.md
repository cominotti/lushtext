## ADDED Requirements

### Requirement: Native minimap highlight remains rendered-pixel stable after width reflow
The minimap SHALL preserve the existing native `GtkSourceMap` viewport highlight effect after sidebar visibility changes, width-only editor reallocations, word-wrap reflow, dynamic overscroll refreshes, and top-of-document anchoring. The system MUST keep the native highlight's rendered top edge, fill, border, neutral styling, interaction behavior, and marker layering; it MUST NOT satisfy this requirement by replacing the native highlight with an app-owned visible overlay. After layout and native source-map frame work settle, screenshot-derived native-highlight anchors SHALL remain stable according to the visual invariant manifest.

#### Scenario: Sidebar hide preserves rendered top anchors at reproduced size
- **WHEN** the minimap is enabled for a supported document at the top of the file
- **AND** word wrap is enabled
- **AND** the window uses the reproduced intermediate geometry around `1822x1272`
- **AND** the workspace sidebar is hidden from its fully shown state
- **THEN** after final sidebar, editor, and minimap allocation settles, the screenshot-derived native viewport top edge remains at the same window-relative y position
- **AND** the screenshot-derived first rendered minimap content row remains at the same window-relative y position
- **AND** app-computed minimap geometry alone cannot satisfy the scenario if the rendered pixels drift

#### Scenario: Sidebar show preserves rendered top anchors at reproduced size
- **WHEN** the minimap is enabled for a supported document at the top of the file
- **AND** word wrap is enabled
- **AND** the window uses the reproduced intermediate geometry around `1822x1272`
- **AND** the workspace sidebar is shown from its fully hidden state
- **THEN** after final sidebar, editor, and minimap allocation settles, the screenshot-derived native viewport top edge remains at the same window-relative y position
- **AND** the screenshot-derived first rendered minimap content row remains at the same window-relative y position
- **AND** the native highlight remains the same neutral `GtkSourceMap` effect rather than a replacement overlay

#### Scenario: Conventional sizes remain controls, not substitutes
- **WHEN** the minimap/sidebar visual matrix runs at conventional sizes such as 720p, 1080p, 1440p, or `1600x1000`
- **THEN** those cases verify the same native rendered-highlight behavior for their geometry
- **AND** passing those sizes does not remove the requirement to run the reproduced intermediate-size case

#### Scenario: Wrap and theme controls preserve the native effect
- **WHEN** the minimap is visible and the sidebar is shown or hidden after layout settles
- **AND** the scenario uses dark theme, light theme, word-wrap enabled, or word-wrap disabled controls
- **THEN** the native viewport highlight remains rendered-pixel stable for that state
- **AND** semantic minimap markers remain visible above or beside the native highlight according to their existing layering contract

### Requirement: Native minimap diagnostics explain rendered slider position
The minimap SHALL expose bounded diagnostic geometry that can explain the native `GtkSourceMap` slider position without becoming the pass/fail oracle for rendered pixels. Diagnostics SHALL include an upstream-informed native-slider estimate derived from public text-view geometry, the map's own visible rect or adjustment state, final allocation, and app-vs-rendered comparison results when screenshot anchors are available.

#### Scenario: Diagnostic estimate includes map visible state
- **WHEN** Automation1 visual geometry is requested while the minimap is visible
- **THEN** the minimap diagnostics include bounded source-map allocation, editor visible-rect summary, map visible-rect or adjustment summary, native-slider estimate, and existing line-projection anchors
- **AND** the diagnostics do not expose document text or minimap-rendered text

#### Scenario: Rendered disagreement is diagnostic failure evidence
- **WHEN** app-computed native-slider diagnostics report a stable y position
- **AND** screenshot-derived native-highlight pixels move outside the declared tolerance
- **THEN** the visual artifact records an app-vs-rendered disagreement
- **AND** the product invariant fails until the rendered pixels are stable
