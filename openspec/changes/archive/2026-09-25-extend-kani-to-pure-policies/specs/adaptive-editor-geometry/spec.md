## ADDED Requirements

### Requirement: Adaptive shell policy invariants are machine-checked
The project SHALL keep Kani harnesses over the production adaptive-shell
policy in `ui/window/geometry/policy.rs`. Their domain SHALL be every `i32`
window width, every workspace preset, every combination of requested
visibility and Focus Mode, and every compact-surface choice. The harnesses
SHALL prove that `derive_adaptive_shell_layout`:

- renders no secondary surface while Focus Mode is active;
- never renders a surface that is not requested;
- renders at most one secondary surface in the compact presentation;
- renders every requested surface in the wide presentation, above the
  workspace breakpoint and outside Focus Mode;
- chooses the sheet presentation exactly when the window width is at or below
  its derived breakpoint threshold.

The policy SHALL be whole-pixel: it takes widths and returns widths and pane
shares (a pane width and the width it is a share of) in whole sp, and the
split-view fraction SHALL be formed once, in the GTK adapter. The harnesses
SHALL also prove, for every workspace width from 0 to 440 sp, that
`properties_breakpoint_max_width_sp` never decreases as the width grows, and
that the threshold is at least the editor-content minimum, plus the layout
overhead, the workspace width, and the properties minimum. For every `i32`
width, every pane share SHALL have a positive width and a positive
denominator, and a properties share taken of the inner split SHALL have a
denominator narrower than the window; the adapter's split fraction SHALL be
tested to be finite and in (0, 1]. No function in the module SHALL panic.

#### Scenario: Compact presentation shows one secondary surface
- **WHEN** the harness explores every input for which document properties use the sheet presentation
- **THEN** the workspace sidebar and document properties are never both rendered

#### Scenario: Focus Mode suppresses every surface
- **WHEN** Focus Mode is active, for any width and requested visibility
- **THEN** neither secondary surface is rendered

### Requirement: Side-by-side preview width is bounded by one third of the content width above its floor
The side-by-side Markdown preview SHALL resolve its width from the preferred
width and the available content width. A preferred width of zero or less
resolves to the 300 sp default. An available width of zero or less counts as
1 sp. The resolved width SHALL satisfy all of these:

- it is at least the 1 sp floor;
- when the available width is at least 3 sp, it is at most one third of the
  available width;
- when the available width is below 3 sp, it equals the 1 sp floor;
- when the preferred width lies between the floor and that one-third bound, it
  equals the preferred width.

The resolution SHALL be a GTK-free policy function that takes and returns
whole pixels (`i32`), with no floating-point arithmetic inside it; the window
SHALL convert the result to `f64` once, where it sets the split view's
constraints. Kani harnesses SHALL prove these properties over every `i32`
preferred and available width. A kept
`should_panic` harness SHALL show that the unconditional "at most one third"
form is false below 3 sp.

#### Scenario: Normal widths respect the one-third bound
- **WHEN** the available content width is 900 sp and the preferred width is 500 sp
- **THEN** the preview width is at most 300 sp

#### Scenario: A tiny content width keeps the non-zero floor
- **WHEN** the available content width is 2 sp
- **THEN** the preview width is the 1 sp floor, which exceeds one third of the available width
