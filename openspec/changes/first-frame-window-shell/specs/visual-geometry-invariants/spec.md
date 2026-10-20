## ADDED Requirements

### Requirement: Staged window composition causes no visible layout jump
The visual-geometry lane SHALL include a same-session scenario that captures the first-frame shell
and the fully composed window of one process, both at the restored window size. Its invariant,
`first-frame-shell-chrome`, SHALL protect these regions with zero pixel difference:
- the header bar;
- the tab bar;
- the status bar;
- the right edge of the workspace rail;
- the left edge of the document-properties pane when it is restored open.

The editor content, rail content, and pane content regions SHALL be declared as allowed to change,
with geometry-anchor assertions on their bounds. The scenario SHALL cover a wide window with both
secondary surfaces requested, a medium window, and a compact window. Changes to the shell template,
the composition stages, or the shell plan SHALL require this invariant in the root summary's
`pixel_verified_invariant_ids`.

#### Scenario: Chrome is identical before and after composition
- **WHEN** the scenario captures the shell's first frame and the composed window's settled frame
- **THEN** every protected chrome region is pixel-identical, and the allowed regions keep their recorded bounds

#### Scenario: Compact restore stays stable
- **WHEN** the scenario runs at a width where the workspace sidebar is collapsed
- **THEN** no rail edge appears in either capture, and the protected chrome regions are pixel-identical
