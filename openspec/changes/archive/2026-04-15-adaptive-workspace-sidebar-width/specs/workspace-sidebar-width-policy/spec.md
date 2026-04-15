## ADDED Requirements

### Requirement: Workspace sidebar width selection lives in Preferences
The system SHALL expose workspace sidebar width selection in `Preferences > Workspace` as a single-choice preference with exactly three options: `Small`, `Comfy`, and `Large`. The selected option MUST reflect the active workspace sidebar preset, and changing the selection MUST apply the new preset immediately.

#### Scenario: Preferences show the sidebar width setting
- **WHEN** the user opens `Preferences > Workspace`
- **THEN** the dialog shows a workspace sidebar width preference
- **AND** the preference offers exactly `Small`, `Comfy`, and `Large`
- **AND** the current selection matches the active workspace sidebar preset

#### Scenario: Selecting a new preset updates the sidebar immediately
- **WHEN** the user changes the workspace sidebar width preference from one preset to another while the workspace sidebar is visible
- **THEN** the workspace sidebar width updates without requiring an app restart
- **AND** the selected preset becomes the new active preset for subsequent layout calculations

### Requirement: Sidebar chrome does not duplicate the width control
The system SHALL keep workspace sidebar width selection in Preferences only. The workspace sidebar itself MUST NOT show a persistent `Small`, `Comfy`, or `Large` control row once the preference-driven control is available.

#### Scenario: Sidebar no longer shows width buttons
- **WHEN** the main window renders the workspace sidebar after this change
- **THEN** the sidebar does not show a fixed footer row containing `Small`, `Comfy`, or `Large` controls

### Requirement: Workspace sidebar presets use adaptive clamped widths
The system SHALL compute the visible workspace sidebar width from the selected preset using a preset-specific hint fraction and preset-specific minimum and maximum widths in scale-independent pixels (`sp`). The preset policies MUST be:

- `Small`: hint `20%`, minimum `220sp`, maximum `280sp`
- `Comfy`: hint `30%`, minimum `280sp`, maximum `360sp`
- `Large`: hint `40%`, minimum `340sp`, maximum `440sp`

The visible width MUST be calculated as:

`clamp(window_width_sp * hint_fraction, min_width_sp, max_width_sp)`

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

### Requirement: Adaptive sidebar widths remain deterministic and persistent
The system SHALL persist the selected workspace sidebar preset across launches. Existing stored sidebar-width values that do not exactly match a supported preset MUST resolve to the nearest supported preset before the adaptive width policy is applied.

#### Scenario: Selected preset is restored on restart
- **WHEN** the user selects a workspace sidebar width preset, closes the app, and reopens it
- **THEN** the same preset is restored
- **AND** the workspace sidebar reuses that preset's adaptive width policy

#### Scenario: Existing stored value snaps to the nearest preset
- **WHEN** an existing installation restores a stored workspace sidebar width value of `0.25`
- **THEN** the app resolves that value to the `Comfy` preset
- **AND** the workspace sidebar applies the `Comfy` adaptive width policy

### Requirement: Dependent split-view math uses the effective sidebar width
The system SHALL derive the workspace split-view fraction, the right properties-pane fraction adjustments, and the properties-pane breakpoint guard from the workspace sidebar's effective visible width after adaptive clamping, rather than from the preset's unclamped hint fraction alone.

#### Scenario: Ultrawide layout uses the clamped effective left width
- **WHEN** the main window width is `2000sp`, the selected workspace preset is `Comfy`, and the workspace sidebar is consuming layout width
- **THEN** downstream split-view calculations use `360sp / 2000sp` as the effective left fraction
- **AND** the properties-pane breakpoint guard does not behave as if the left pane were still consuming `30%` of the total window width

#### Scenario: Changing the preset recalculates the right-pane guard
- **WHEN** the user changes the workspace sidebar width preset while the properties pane is available in the same window shell
- **THEN** the properties-pane width calculation and breakpoint guard recalculate from the newly effective workspace sidebar width
