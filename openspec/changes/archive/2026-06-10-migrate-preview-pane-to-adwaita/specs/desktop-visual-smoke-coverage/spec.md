## ADDED Requirements

### Requirement: Visual smoke covers both Markdown preview presentations
The visual smoke lane SHALL include real-session proof for both preview-only
Markdown rendering and the side-by-side Markdown preview surface whenever a
change modifies the preview shell or its geometry-sensitive template nodes. The
side-by-side proof MUST use documented preview target-state actions and bounded
automation state before accepting screenshots as evidence.

#### Scenario: Side-by-side preview smoke verifies state before capture
- **WHEN** a visual smoke scenario captures the side-by-side Markdown preview surface
- **THEN** it opens a Markdown fixture through the normal document path
- **AND** it requests side-by-side preview through the documented target-state action
- **AND** it verifies `surfaces.preview_pane_visible` and `surfaces.preview_mode` through automation before accepting the screenshot

#### Scenario: Preview-only and side-by-side captures remain distinct
- **WHEN** the visual smoke lane captures Markdown preview states for a preview-shell migration
- **THEN** one scenario proves preview-only mode with `surfaces.preview_mode=true`
- **AND** another scenario proves side-by-side preview with `surfaces.preview_pane_visible=true` and `surfaces.preview_mode=false`
- **AND** the artifacts distinguish compact and wide presentation when both are exercised

#### Scenario: Preview shell warnings fail visual smoke
- **WHEN** side-by-side preview or preview-only smoke captures finish
- **THEN** unexpected GTK, Libadwaita, GDK, renderer, and accessibility warnings emitted by the preview shell fail the lane
- **AND** the warning scan preserves logs alongside the screenshot and automation snapshot artifacts
