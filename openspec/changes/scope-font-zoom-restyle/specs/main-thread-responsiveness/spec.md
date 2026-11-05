## ADDED Requirements

### Requirement: A zoom step restyles only the widgets that follow the editor font
A change of `zoom-level` SHALL NOT change the rules of any style provider
while the base font (custom or desktop family and size, and whether the system
font is used) is unchanged and the new level is one the provider already
carries. It SHALL instead change one CSS class on each widget that follows the
editor font (every editor surface view and every application widget carrying
`.monospace`), inside the paced restyle of dense documents. A change of the
base font MAY reload the provider once; that whole-tree revalidation SHALL be
recorded as an exception against ledger A86 with its measured cost. The
interaction dispatch of a zoom step SHALL NOT grow with the number of widgets
that do not follow the font.

#### Scenario: A zoom step reloads no provider
- **WHEN** the user zooms in from 100 % to 110 % with the custom font unchanged
- **THEN** the font provider's rules are unchanged, and every registered
  widget carries the 110 % class and none other

#### Scenario: Unrelated widgets do not add to a zoom step
- **WHEN** the user zooms on a dense document with the workspace sidebar shown
  and populated with 300 entries
- **THEN** the zoom step's interaction dispatch is within one millisecond of
  the same step with the sidebar hidden

#### Scenario: A font change reloads the provider once
- **WHEN** the user picks another custom font
- **THEN** the provider is reloaded once, with the zoom rules of the new font,
  and every registered widget keeps its zoom class
