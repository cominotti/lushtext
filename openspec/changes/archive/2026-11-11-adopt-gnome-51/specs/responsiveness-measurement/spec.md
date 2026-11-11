## ADDED Requirements

### Requirement: A runtime move is accepted only on a passing lane on the shipped toolkit
Before the Flatpak manifest's GNOME runtime changes, the responsiveness lane's quick tier SHALL pass on the toolkit that runtime ships. The run SHALL have no withheld typing frames, no typing that reached no document, no failed cases other than a documented environment skip, and no `GtkSourceMapSlider` or other toolkit warnings. Each failure the toolkit change introduces SHALL be attributed to the toolkit, the compositor (mutter) or LushText, with an isolated probe or the probe's frame-clock records, and fixed at its cause. The lane's failure rules SHALL NOT be loosened to make it pass. A regression that cannot be fixed in LushText SHALL be recorded with its evidence and decided with the maintainer, user experience first, before the runtime is shipped.

#### Scenario: Typed text drawn late on GTK 4.24
- **WHEN** the quick tier runs `baseline/small` on GTK 4.24 / mutter 51 and keys typed into the editor are first drawn only after the next key
- **THEN** the run fails with those phases listed under `withheld_typing_frames`
- **AND** the change that moves the runtime attributes and fixes the cause before the manifest change is accepted

#### Scenario: Typing that misses the document
- **WHEN** the keys of a typing phase arrive outside an editor or insert nothing on the new toolkit
- **THEN** the run fails and the cause (click point, focus, or held input) is fixed in the same change

#### Scenario: A case that fails
- **WHEN** a measured repetition ends with outcome `failed` (a phase could not run)
- **THEN** the run fails with the repetition listed under `failed_cases`
- **AND** a case that opens no document records its editor interactions as unavailable instead of aiming a click at an editor
