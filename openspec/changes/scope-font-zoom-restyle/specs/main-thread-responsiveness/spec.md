## ADDED Requirements

### Requirement: A zoom step restyles only the widgets that follow the editor font
A change of `zoom-level` SHALL NOT change the rules of any style provider
while the base font (custom or desktop family and size, and whether the system
font is used) is unchanged and the new level is one the provider already
carries. It SHALL instead change one CSS class on each widget that follows the
editor font (every editor surface view and every application widget carrying
`.monospace`), in every window, inside the paced restyle of dense documents.
A change of the base font, or a zoom level on a step lattice the provider does
not carry yet, MAY load one provider once; it SHALL do so in a dispatch that
changes no widget's class, and swap the classes in a later dispatch, so the
whole-tree revalidation and the re-layout of the font-following widgets do not
share a dispatch. Where that revalidation exceeds `B_SETTLE_US` it SHALL be
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
  the same step in the same window geometry with the sidebar shown over an
  empty folder

#### Scenario: A font change reloads the provider once
- **WHEN** the user picks another custom font
- **THEN** one provider is loaded once, with the zoom rules of the new font,
  in a dispatch that changes no widget's class
- **AND** in a later dispatch every registered widget moves to the new font's
  class for the same zoom level and carries no other zoom class

#### Scenario: A zoom level set from outside the UI reloads once
- **WHEN** `zoom-level` is set to 55 from outside the UI and the user then
  zooms in twice
- **THEN** the provider is loaded once, for the 55 % lattice, and the two
  steps to 65 % and 75 % load no provider

#### Scenario: A zoom step reaches every window
- **WHEN** two windows are open and the user zooms in one of them
- **THEN** the editors and registered widgets of both windows carry the new
  zoom class, and no provider is loaded
