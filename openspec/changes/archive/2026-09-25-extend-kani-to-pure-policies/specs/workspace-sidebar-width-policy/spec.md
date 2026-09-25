## MODIFIED Requirements

### Requirement: Workspace sidebar presets use adaptive clamped widths
The system SHALL compute the visible workspace sidebar width from the selected preset using a preset-specific hint percentage and preset-specific minimum and maximum widths in whole scale-independent pixels (`sp`). The preset policies MUST be:

- `Small`: hint `20%`, minimum `220sp`, maximum `280sp`
- `Comfy`: hint `30%`, minimum `280sp`, maximum `360sp`
- `Large`: hint `40%`, minimum `340sp`, maximum `440sp`

The visible width MUST be calculated in whole sp, with the product floored:

`clamp(floor(window_width_sp * hint_percent / 100), min_width_sp, max_width_sp)`

The width policy SHALL do no floating-point arithmetic; the split-view fraction the window needs (the visible width divided by the window width, capped at 1) SHALL be formed once, in the GTK adapter.

#### Scenario: Comfy keeps the current default-window feel
- **WHEN** the main window width is `1200sp` and the selected preset is `Comfy`
- **THEN** the visible workspace sidebar width is `360sp`

#### Scenario: Comfy stops growing on ultrawide windows
- **WHEN** the main window width is `2000sp` and the selected preset is `Comfy`
- **THEN** the visible workspace sidebar width is `360sp`
- **AND** the workspace sidebar does not expand to `600sp`

#### Scenario: Small still respects a comfortable minimum on desktop widths above collapse
- **WHEN** the main window width is `900sp` and the selected preset is `Small`
- **THEN** the visible workspace sidebar width is `220sp`

#### Scenario: Large remains bounded on wide windows
- **WHEN** the main window width is `1400sp` and the selected preset is `Large`
- **THEN** the visible workspace sidebar width is `440sp`
- **AND** the workspace sidebar does not expand to `560sp`

#### Scenario: A fractional product is floored to whole sp
- **WHEN** the main window width is `1001sp` and the selected preset is `Comfy`
- **THEN** the visible workspace sidebar width is `300sp`

### Requirement: Adaptive sidebar widths remain deterministic and persistent
The system SHALL persist the selected workspace sidebar preset across launches. Existing stored sidebar-width values that do not exactly match a supported preset MUST resolve to the nearest supported preset before the adaptive width policy is applied, by comparison only: a value below `0.25` resolves to `Small`, a value above `0.35` resolves to `Large`, and a value from `0.25` to `0.35` inclusive resolves to `Comfy`, so a value exactly at a midpoint resolves to `Comfy`. A stored value that is not finite (NaN or infinite) MUST resolve to the default preset, `Comfy`.

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

- `clamped_width_sp` equals `clamp(floor(max(window_width, 1) * hint_percent /
  100), min_width_sp, max_width_sp)`, in whole sp;
- `clamped_width_sp` lies within the preset's minimum and maximum;
- `clamped_width_sp` never decreases as the window width grows;
- the integer hint percentage equals the stored hint fraction times 100.

For every preset they SHALL also prove that `from_index(index())` and
`from_fraction(fraction())` return that preset, and that `from_index` of any
value from 3 up returns nothing. For every non-finite value, `from_fraction`
SHALL return the default preset; for finite values it SHALL never resolve to a
narrower preset as the stored value grows, SHALL resolve each midpoint to
`Comfy`, and, for every stored value of magnitude at most 2, SHALL pick a
preset at least as near as any other. A property test SHALL check the
comparison form against the nearest-delta form it replaced. The split-view
fraction formed in the GTK adapter SHALL be tested to be finite and in (0, 1]
for every preset and `i32` window width.

#### Scenario: Clamp stays within preset bounds
- **WHEN** the harness explores every `i32` window width for every preset
- **THEN** the visible width is never below the preset minimum or above its maximum

#### Scenario: Round-trip is exact
- **WHEN** a preset is stored as its hint fraction and read back
- **THEN** the same preset is restored
