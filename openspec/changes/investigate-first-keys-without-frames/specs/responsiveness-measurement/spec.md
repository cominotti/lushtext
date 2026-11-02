## ADDED Requirements

### Requirement: The lane proves its inputs reached the document
The measurement build's probe SHALL stamp each key and button input record with where it was headed: the window's focus widget for a key, and the widget under the pointer (with its window coordinates) for a button, and SHALL record each pointer motion with its window coordinates. The target is classed as an editable editor, a read-only editor, another widget, or none. The probe SHALL record a mark after each insertion an editor buffer makes inside a user action. Without the `responsiveness-probes` feature none of this SHALL be compiled in.

The lane SHALL aim every click meant for the editor at a point of the editor text view's rectangle, from the automation snapshot, that no sidebar or bottom sheet covers. It SHALL correct the pointer by where the probe saw it arrive. It SHALL fail the click's phase unless the probe's record of the press names an editor. For each phase that types text keys, it SHALL report how many keys were typed, how many left no input record, where they arrived and how many insertions they made. A phase with a text key that left no input record, whose text keys arrived outside an editor, or that inserted fewer characters than the keys it typed into an editable editor, SHALL fail the run and SHALL be listed in the report. A build whose probe records no targets SHALL be reported as unverified, not failed, for where its recorded keys arrived; its keys without any input record SHALL still fail the phase.

A text key that reached an editable editor SHALL NOT be classed as changing nothing visible. When the first frame after it starts only after the next key was handled while the main loop was free, or after the phase ended, its latency SHALL be measured to that frame, the phase SHALL be listed as a withheld typing frame, and the run SHALL fail. Keys into a read-only (protected) editor SHALL be reported apart from editable typing and SHALL NOT enter its input-to-frame figures. After a relaunch, a driver click that cannot be proven to reach the editor SHALL end the case with a driver failure.

The lane SHALL seed every settings key the case's layout depends on, including `split-view-layout-migrated`, so that the layout it records is the layout rendered. The fresh construction profiles that show no pane SHALL instead seed the legacy `sidebar-visible` as hidden, unmigrated, so that their launch runs the one-shot split-view migration and renders the same layout.

#### Scenario: A focus click lands outside the editor
- **WHEN** a driver click meant for the editor arrives on another widget
- **THEN** its phase fails with the widget class and the window position it arrived at

#### Scenario: Typing reaches no document
- **WHEN** a typing phase's keys arrive at a widget that is not an editor, or insert nothing in an editable editor
- **THEN** the report lists the phase under `misdirected_typing` and the run fails

#### Scenario: A baseline without targets stays measurable
- **WHEN** the measured binary's probe records no input targets
- **THEN** the delivery is reported as unverified and the run is not failed for it

#### Scenario: Keys that never reached the application
- **WHEN** a typing phase's text keys leave no input record at all
- **THEN** the phase is listed under `misdirected_typing` and the run fails

#### Scenario: Typed text drawn in a withheld frame
- **WHEN** a key typed into an editable editor is drawn only in a frame that starts after the next key was handled while the main loop was free
- **THEN** its latency runs to that frame, the phase is listed under `withheld_typing_frames`, and the run fails
