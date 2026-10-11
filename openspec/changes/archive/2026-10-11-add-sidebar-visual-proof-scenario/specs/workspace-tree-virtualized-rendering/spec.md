## ADDED Requirements

### Requirement: A revealed path reaches its row through user navigation paths

A workspace path reveal SHALL reach its row through the same bounded paths user navigation uses: ancestor expansion through the section's admitted directory scans, selection through the section's `SingleSelection`, and scrolling through `GtkListView::scroll_to` forwarded by the `ViewportSliceBin`. It SHALL NOT scroll the outer sidebar scroller directly, realize rows outside the viewport band, or bypass the per-directory entry cap. After a reveal the section's rendered rows SHALL satisfy the same rendered-bounds contract as after keyboard navigation: rows drawn contiguously, the revealed row drawn wholly inside the outer viewport, and every row drawn at the same position across repeated forced layouts.

#### Scenario: Reveal beyond the cap keeps realized rows bounded
- **WHEN** a reveal targets row 399 of a 400-row directory
- **THEN** the number of realized row widgets stays bounded by the viewport, and the revealed row is drawn inside the outer viewport

#### Scenario: Reveal comes to rest
- **WHEN** a reveal has reported `revealed`
- **THEN** six forced layouts draw the revealed row at one position, the outer scroller value does not change, and the slice bin's `correction_count()` does not grow

#### Scenario: Reveal of an already visible row does not scroll
- **WHEN** the target row is already drawn inside the viewport at rest
- **THEN** the reveal selects it without moving the outer scroller, and the workspace section header is drawn where it was
