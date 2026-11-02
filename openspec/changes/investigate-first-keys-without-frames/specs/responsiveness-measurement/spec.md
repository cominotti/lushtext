## ADDED Requirements

### Requirement: The lane proves its inputs reached the document
The measurement build's probe SHALL stamp each key and button input record with where it was headed: the window's focus widget for a key, and the widget under the pointer (with its window coordinates) for a button, and SHALL record each pointer motion with its window coordinates. The target is classed as an editable editor, a read-only editor, another widget, or none. The probe SHALL record a mark after each insertion an editor buffer makes inside a user action. Without the `responsiveness-probes` feature none of this SHALL be compiled in.

The lane SHALL aim every click meant for the editor at a point of the editor text view's rectangle, from the automation snapshot, that no sidebar or bottom sheet covers. It SHALL correct the pointer by where the probe saw it arrive. It SHALL fail the click's phase unless the probe's record of the press names an editor. For each phase that types text keys, it SHALL report how many keys were typed, where they arrived and how many insertions they made. A phase whose text keys arrived outside an editor, or that inserted fewer characters than the keys it typed into an editable editor, SHALL fail the run and SHALL be listed in the report. A build whose probe records no targets SHALL be reported as unverified, not failed.

The lane SHALL seed every settings key the case's layout depends on, including `split-view-layout-migrated`, so that the layout it records is the layout rendered.

#### Scenario: A focus click lands outside the editor
- **WHEN** a driver click meant for the editor arrives on another widget
- **THEN** its phase fails with the widget class and the window position it arrived at

#### Scenario: Typing reaches no document
- **WHEN** a typing phase's keys arrive at a widget that is not an editor, or insert nothing in an editable editor
- **THEN** the report lists the phase under `misdirected_typing` and the run fails

#### Scenario: A baseline without targets stays measurable
- **WHEN** the measured binary's probe records no input targets
- **THEN** the delivery is reported as unverified and the run is not failed for it
