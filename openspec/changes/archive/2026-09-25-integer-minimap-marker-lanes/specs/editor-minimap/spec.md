## ADDED Requirements

### Requirement: Semantic marker lanes sit on whole pixels
The minimap marker strip SHALL draw each semantic marker category in its own
right-anchored lane whose width is a fixed share of the strip width: bookmarks
100%, search matches 82%, modified-since-save regions 64%, and long-line
warnings 46%. Each lane width SHALL be rounded to the nearest whole pixel, with
ties rounded up, and SHALL be at least two pixels. A lane's left edge SHALL be
the strip width minus the lane width. The lane geometry SHALL be computed in
integers in the minimap's pure policy and converted to fractional coordinates
only at the drawing call.

#### Scenario: Lanes on the shipped strip
- **WHEN** the marker strip is 8 px wide
- **THEN** the bookmark, search, modified, and long-line lanes are 8, 7, 5, and 4 px wide
- **AND** their left edges are at 0, 1, 3, and 4 px, so every lane ends at the strip's right edge

#### Scenario: Lanes stay nested inside the strip
- **WHEN** the strip is at least 4 px wide
- **THEN** each lane is within half a pixel of its share, no wider than the strip, and no wider than the lane of the category before it
