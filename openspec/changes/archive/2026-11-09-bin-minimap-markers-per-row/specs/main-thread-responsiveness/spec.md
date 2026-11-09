## ADDED Requirements

### Requirement: Minimap marker dispatches fit the interaction budgets at any marker count
On the reference host, with the minimap visible, every dispatch the minimap
owns (a marker refresh, a scan-completion re-arm, a tag-retirement slice) and
the minimap's own marker work inside any other dispatch (the strip's draw
function with its row table and bins, the tag application in an edit's
handlers) SHALL fit `B_INTERACT_US`. The frame dispatch that shows the
change SHALL fit `B_SETTLE_US` and stay within the base revision's figure
plus one reference frame. This covers:

- an edit, a search change, a scan completion, a save, or a scroll of the
  editor or the map;
- documents up to the minimap's size tier;
- up to 131,072 markers of each kind;
- strips of 713 and 1,425 rows.

The marker work in these dispatches SHALL NOT grow with the number of
markers. Measurements SHALL be interleaved with the base revision at a
1-minute load below 4, with the load recorded before and after. A dispatch
that cannot meet its budget because of the toolkit (for example the
`GtkSourceMap`'s own relayout of edited lines) SHALL be recorded as a
toolkit-floor exception, with a GTK axiom probe, its measured figures and a
widget assertion bounding it. It SHALL NOT be left unexplained.

#### Scenario: Scrolling a densely marked document
- **WHEN** the user scrolls a document with 131,072 search matches, 131,072 modified runs and its long-line markers enabled
- **THEN** every scroll frame's marker work fits `B_INTERACT_US`, at the same cost as with 2,048 markers of each kind

#### Scenario: Saving a heavily modified document
- **WHEN** the user saves a document with 131,072 separate modified runs
- **THEN** no dispatch of the save terminal or of the tracking retirement exceeds `B_INTERACT_US`
