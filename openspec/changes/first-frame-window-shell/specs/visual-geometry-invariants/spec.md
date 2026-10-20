## ADDED Requirements

### Requirement: Staged window composition causes no visible layout jump
The visual-geometry lane SHALL include a same-session scenario that captures the first presented
frame and the fully composed window of one process, both at the restored window size. Capturing
the first frame MAY use a test-only hold after it; the hold SHALL NOT exist in shipped builds. Its
invariant, `first-frame-shell-chrome`, SHALL protect these regions with zero pixel difference:
- the header bar;
- the tab bar;
- the status bar;
- the right edge of the workspace rail;
- the left edge of the document-properties pane when it is restored open.

The editor content, rail content, and pane content regions SHALL be declared as allowed to change,
with geometry-anchor assertions on their bounds. The scenario SHALL cover a wide window with both
secondary surfaces requested, a medium window, and a compact window. Changes to the composition
stages or to the visibility defaults of staged content SHALL require this invariant in the root
summary's `pixel_verified_invariant_ids`.

Because two captures cannot show the frames between them, a deterministic widget test SHALL also
record, at every `after-paint` from the first frame until composition completes and without any
hold, the bounds of the header bar, tab bar and status bar, the Open button's visible form, and
the rail and pane edges, and SHALL fail if any of them changes.

#### Scenario: Chrome is identical before and after composition
- **WHEN** the scenario captures the first frame and the composed window's settled frame
- **THEN** every protected chrome region is pixel-identical, and the allowed regions keep their recorded bounds

#### Scenario: Compact restore stays stable
- **WHEN** the scenario runs at a width where the workspace sidebar is collapsed
- **THEN** no rail edge appears in either capture, and the protected chrome regions are pixel-identical

#### Scenario: No intermediate frame moves the chrome
- **WHEN** the per-frame trace runs over an unheld composition at a width where the Open-button breakpoint applies
- **THEN** every recorded chrome bound and the Open button's form are the same at every painted frame
