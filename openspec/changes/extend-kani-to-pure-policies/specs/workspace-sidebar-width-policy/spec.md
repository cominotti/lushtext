## MODIFIED Requirements

### Requirement: Adaptive sidebar widths remain deterministic and persistent
The system SHALL persist the selected workspace sidebar preset across launches. Existing stored sidebar-width values that do not exactly match a supported preset MUST resolve to the nearest supported preset before the adaptive width policy is applied. When presets are equally near (within `f64::EPSILON`), the value MUST resolve by the tie order `Comfy`, then `Small`, then `Large`. A stored value that is not finite (NaN or infinite) MUST resolve to the default preset, `Comfy`.

#### Scenario: Selected preset is restored on restart
- **WHEN** the user selects a workspace sidebar width preset, closes the app, and reopens it
- **THEN** the same preset is restored
- **AND** the workspace sidebar reuses that preset's adaptive width policy

#### Scenario: Existing stored value snaps to the nearest preset
- **WHEN** an existing installation restores a stored workspace sidebar width value of `0.25`
- **THEN** the app resolves that value to the `Comfy` preset
- **AND** the workspace sidebar applies the `Comfy` adaptive width policy

#### Scenario: A non-finite stored value falls back to the default
- **WHEN** the stored workspace sidebar width value is NaN or negative infinity
- **THEN** the app resolves it to the `Comfy` preset rather than to `Large`

## ADDED Requirements

### Requirement: Preset clamps and round-trips are machine-checked
The project SHALL keep Kani harnesses over `WorkspaceSidebarWidthPreset`. For
every preset and every `i32` window width (a width of zero or less counts as 1
sp), they SHALL prove:

- `clamped_width_sp` equals `clamp(max(window_width, 1) * hint_fraction,
  min_width_sp, max_width_sp)`;
- `clamped_width_sp` lies within the preset's minimum and maximum;
- `clamped_width_sp` never decreases as the window width grows;
- `effective_fraction` is finite and in (0, 1].

For every preset they SHALL also prove that `from_index(index())` and
`from_fraction(fraction())` return that preset, and that `from_index` of any
value from 3 up returns nothing. For every finite `f64` they SHALL prove that
`from_fraction` returns a preset whose hint fraction is within `f64::EPSILON` of
the nearest, following the tie order above. For every non-finite value, `from_fraction` SHALL return the
default preset.

#### Scenario: Clamp stays within preset bounds
- **WHEN** the harness explores every `i32` window width for every preset
- **THEN** the visible width is never below the preset minimum or above its maximum

#### Scenario: Round-trip is exact
- **WHEN** a preset is stored as its hint fraction and read back
- **THEN** the same preset is restored
