## ADDED Requirements

### Requirement: A reflow repair never undoes a scroll made during the burst
The settled repair of a passive width reflow SHALL restore the top edge only when the viewport has not been scrolled since the burst began. A scroll the user, the application or GTK's own navigation makes while the burst is pending (opening at a line, search navigation, a scroll wheel, a direct `GtkTextView` mark scroll) MUST survive the repair, and the editor's rest state MUST then record that it is no longer at the top. A GTK-preserved offset at the top MUST still return to the top.

#### Scenario: Scroll while the minimap appears or hides
- **WHEN** the editor rests at the top of a 2,000-line document
- **AND** a passive width reflow begins (the minimap is hidden or shown)
- **AND** the view is scrolled to line 1,000 before the burst settles
- **THEN** after the settle and a further 600 ms the editor still shows line 1,000

#### Scenario: A kept scroll survives later reflows
- **WHEN** a scroll to line 1,000 made during a passive width burst was kept by the settle
- **AND** a later width reflow and a later height-only change of the editor each settle
- **THEN** the editor still shows line 1,000 after each of them

#### Scenario: Scroll on a starved main loop
- **WHEN** the minimap overscroll test runs with its process and two busy loops pinned to one CPU
- **THEN** its "stays scrolled to the middle" precondition holds on 20 consecutive isolated runs

#### Scenario: Top anchor still repaired after reflow
- **WHEN** the editor rests at the top and only a width reflow moves the vertical value
- **THEN** after the settle the editor's first visible line is line one
