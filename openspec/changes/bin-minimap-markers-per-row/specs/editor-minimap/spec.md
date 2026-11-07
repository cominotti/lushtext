## MODIFIED Requirements

### Requirement: The minimap shows semantic navigation markers
The system SHALL render visually distinguishable semantic markers in the minimap for editor-local navigation signals. The default supported marker set SHALL include bookmarks, active in-tab search matches, and modified-since-save line regions. Lines above the minimap warning threshold SHALL be represented by long-line warning markers only when the user enables the long-line marker preference. Semantic marker positions SHALL be projected from the same rendered `GtkSourceMap` geometry used by the minimap content, including top margin, dynamic EOF overscroll, and allocation changes. Semantic markers MUST NOT be spread across blank EOF overscroll tail space when their corresponding document lines render above that tail. No marker kind SHALL have a count cap: every bookmark, search match, modified-since-save line and enabled long line on a line the minimap currently shows SHALL be represented in the strip, at any marker count.

#### Scenario: Bookmarks appear in the minimap
- **WHEN** the active editor page contains bookmarks
- **THEN** the minimap shows bookmark markers at the corresponding document regions
- **AND** removing a bookmark removes its minimap marker

#### Scenario: Active search matches appear in the minimap
- **WHEN** the user has an active in-tab search with one or more matches in the editor
- **THEN** the minimap shows search-match markers at the corresponding regions
- **AND** clearing or closing the in-tab search removes those search markers

#### Scenario: Search markers respect EOF overscroll geometry
- **WHEN** the active editor page has dynamic EOF overscroll
- **AND** an active in-tab search produces markers for document lines above the final rendered content line
- **THEN** search-match markers align with the corresponding rendered minimap content regions
- **AND** search-match markers do not extend into the blank EOF overscroll tail below the rendered document content

#### Scenario: All semantic marker categories share source-map projection
- **WHEN** the active editor page shows bookmark, search-match, modified-since-save, and enabled long-line warning markers
- **THEN** each marker category is positioned using the same source-map layout geometry
- **AND** resizing or reallocation refreshes marker positions without changing which semantic lines are marked

#### Scenario: Modified-since-save markers clear after save
- **WHEN** the user edits a document and then saves it successfully
- **THEN** the minimap removes modified-since-save markers for those saved changes
- **AND** later unsaved edits create new modified markers again

#### Scenario: Long-line markers stay hidden by default
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **AND** the minimap is enabled
- **AND** the long-line marker preference is disabled
- **THEN** the minimap does not show long-line warning markers for those regions

#### Scenario: Enabled long-line markers flag long lines
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **AND** the minimap is enabled
- **AND** the long-line marker preference is enabled
- **THEN** the minimap shows long-line warning markers for those regions
- **AND** shortening those lines removes the corresponding warnings

#### Scenario: Disabling long-line markers removes existing warnings
- **WHEN** long-line warning markers are visible in the minimap
- **AND** the user disables the long-line marker preference
- **THEN** the minimap removes long-line warning markers
- **AND** bookmark, search-match, and modified-since-save markers remain governed by their existing behavior

#### Scenario: Matches past the former cap are drawn
- **WHEN** a document has 131,072 search matches on alternating lines and the map is scrolled to its last lines
- **THEN** the strip draws search markers on the rows that show those lines, as it does at the top of the document

#### Scenario: Markers follow the map when it scrolls
- **WHEN** the editor scrolls a document taller than the map, so the map scrolls too
- **THEN** the strip's rendered marker pixels move with the map content in the same frame and match a fresh projection of the visible lines

### Requirement: Minimap fit functions are machine-checked on a stated domain
The project SHALL keep Kani harnesses over the production minimap fit
functions: `fit_native_slider_to_source_map_bounds`, `fit_projected_bounds`,
`expanded_to_min_height`, and `native_slider_estimate_from_inputs`, and over
the integer marker row binning (the row-group walker, the row-run builder and
its whole-pixel minimum-height expansion) that replaced `fit_marker_bounds`.
The harnesses SHALL prove that none of these functions panics for any `f64`
input, including non-finite values, or any `i32` input, and that every
coordinate a function returns is finite. The one exception is the private
`expanded_to_min_height`, whose callers guarantee a finite, non-empty band
that already contains the span: it SHALL be proved on that precondition for
every `f64` minimum height, and a kept `should_panic` harness SHALL show that
it panics on an inverted band.

The fit functions' inputs are GTK widget coordinates and scroll-adjustment
values, which are fractional under scaling and smooth scrolling, so the
functions stay `f64` and their rustdoc states the domain of each property. The
marker row binning takes whole pixels and line numbers only and holds no
floating-point arithmetic. On a whole-pixel domain the harnesses SHALL prove
containment. In that domain every coordinate is an integer with magnitude at
most 2^20, and every minimum height is an integer from 0 to 2^16. They SHALL
prove:

- a returned marker row run or projected rectangle lies inside the
  rendered-content band, `lower <= top < bottom <= upper`, with a positive
  height. Here `lower` is `max(content_top, 0)`, and `upper` is
  `min(content_bottom, h)`, where `h` is the strip height or the target
  height;
- `expanded_to_min_height`, to which the projected fit delegates the minimum
  height, returns a span at least the smaller of the minimum height and the
  band height. This is proved on the smaller whole-pixel domain the solver can
  finish (magnitude at most 2^8, minimum heights from 0 to 2^6), stated in the
  harness and the rustdoc, and a unit test SHALL sample the 2^20 domain; the
  integer row-run expansion SHALL be proved to give the same guarantee on its
  bounded domain;
- `RejectOutside` returns nothing for a span entirely outside the band;
- the fitted native slider keeps its horizontal position and width and lies
  vertically inside the source-map bounds.

When one of these properties fails for general finite `f64` inputs, the
counterexample SHALL be kept as a `should_panic` harness. The function's
rustdoc SHALL then state the whole-pixel domain on which the property is
proved.

#### Scenario: Markers never reach the EOF overscroll tail
- **WHEN** the harness explores every row table, marker-line set and band within its bounded domain
- **THEN** no drawn marker row run's bottom lies below the last rendered content line (`bottom <= min(content_bottom, strip_height)`)

#### Scenario: Non-finite geometry never panics
- **WHEN** any fit function receives NaN or infinite coordinates
- **THEN** it returns without panicking, and it returns no bounds unless every coordinate it returns is finite

## ADDED Requirements

### Requirement: Minimap markers are binned per pixel row
The marker strip SHALL decide what to draw per pixel row of the strip,
inside the rendered-content band. For each row it SHALL read which document
lines the map shows there, through the map's own line geometry, and group
rows that show the same lines. Each marker kind SHALL answer "the first
marker line at or after a line" from its own index. A row group that holds
any marker line of a kind SHALL draw that kind's lane on every pixel row of
the group. Adjacent marked rows of a kind SHALL be drawn as one rectangle.
Rectangles shorter than the minimum marker height SHALL grow to it inside the
band, and rectangles of one kind that then overlap or touch SHALL be merged,
so no pixel of a lane is painted twice. The binning SHALL call each kind's probe at most once per row group
plus once, so its cost depends on the strip height and not on the marker
count. Row tables, runs and the minimum height SHALL be whole pixels in the
pure policy, converted to floating point only at the drawing call. The band
SHALL be rounded outward to whole pixels and clamped to the strip. Bins SHALL
be recomputed when the map scrolls, the strip is resized, the band changes,
or a kind's markers change. The strip SHALL stay decorative and SHALL NOT
become a focus or pointer target.

The pure walker SHALL be proved by Kani on a bounded domain:

- the row table is monotone, and every row is in exactly one group;
- the walker marks exactly the groups whose line range contains a marker
  line, and each marker line inside the band lands in exactly one group;
- no row outside the band is marked;
- it calls the probe at most `groups + 1` times;
- the runs it builds are disjoint, in increasing order, inside the band, and
  at least the smaller of the minimum height and the band height.

#### Scenario: Hidden lines are still represented
- **WHEN** a range of folded, zero-height lines that contains a modified line lies between two strip rows
- **THEN** the row group whose line range contains the folded range draws the modified lane

#### Scenario: Drawing cost does not depend on the marker count
- **WHEN** the same strip draws a document with 2,048 and with 131,072 search matches
- **THEN** each kind's probe is called at most once per row group plus once in both cases

#### Scenario: Marker pixels stay on whole pixels
- **WHEN** a one-pixel marker line is drawn at display scale 1
- **THEN** its rectangle is two whole pixels tall, inside the rendered-content band, with no anti-aliased partial row

### Requirement: Modified-since-save lines are tracked without a count
The system SHALL track modified-since-save lines with a buffer-attached
representation that moves with the text and has no count cap: a `GtkTextTag`
without properties, applied over the lines an insertion or deletion touches.
A restored draft SHALL mark every line of the buffer. A transition to the
saved state, and every load, buffer replacement, local-history restore and
memory eviction that clears the markers today, SHALL clear them in the same
dispatch, by swapping in a fresh tag. The old tag's toggles SHALL be removed
in low-priority, generation-guarded slices, each bounded by a count of
toggles and within `B_INTERACT_US`. A tag still retiring when another swap
happens SHALL stay queued until it is retired. Each retired tag SHALL then be
removed from the tag table, and disposing the editor SHALL cancel the
retirement. Tracking SHALL NOT create undo entries, change
layout, or reach saved, draft or local-history text. Programmatic buffer
replacement SHALL keep suspending tracking exactly as before.

#### Scenario: Edits past the former cap are marked
- **WHEN** the user has edited 2,048 separate lines and edits another line further down
- **THEN** the minimap shows a modified marker for the new edit

#### Scenario: A restored draft shows every line modified
- **WHEN** a draft of 131,072 lines is restored into an editor with the minimap visible
- **THEN** every strip row that shows document lines draws the modified lane, with no sampled gaps

#### Scenario: Saving clears markers at once without a long dispatch
- **WHEN** a document with 131,072 separate modified runs is saved
- **THEN** the modified markers disappear in the save's terminal dispatch
- **AND** no dispatch that removes the old tracking exceeds `B_INTERACT_US`

#### Scenario: Tracking leaves no trace in the document
- **WHEN** the user edits, undoes and saves a tracked document
- **THEN** the undo history holds only the text edits, and the saved bytes are exactly the buffer text

### Requirement: Search markers never force a synchronous scan
The minimap SHALL read search markers from the in-tab search context's
synchronous probe only after the context reports a completed scan. It SHALL
redraw the strip when the scan completes. No minimap dispatch SHALL call a
probe of the live search context that scans unscanned text synchronously. Time to the first
in-band search marker SHALL be measured against the base revision on an
ordinary, a sparse and a dense document. If it is later by more than one
reference frame on any of them, the minimap SHALL also fill the search bins
of the rows it shows before the scan completes, without blocking a dispatch
past `B_INTERACT_US` and without a request a query change can leave
unanswered: it SHALL search a bounded private copy of the text those rows
show, with the same query and options, and use the scan's results once the
scan completes. Where the base revision is earlier only through a dispatch
over `B_INTERACT_US`, the difference SHALL be recorded as an exception with
both figures.

#### Scenario: Sparse matches in a large document
- **WHEN** the user searches a 262,144-line document whose only match is on its last line
- **THEN** no minimap dispatch exceeds `B_INTERACT_US`
- **AND** the search marker appears once the scan completes and the map shows that line

#### Scenario: Every match is drawn after the scan
- **WHEN** the scan of a query with 131,072 matches completes
- **THEN** every strip row that shows a matching line draws the search lane

### Requirement: Long-line markers keep every long line
The minimap's long-line analysis SHALL retain every line above the warning
threshold that it inspects, with no count cap. The retained list is bounded
by the document, at most one entry per line longer than the threshold, and
analysis runs only while the document scale policy keeps the minimap
available. Marker visibility SHALL keep following the long-line preference.

#### Scenario: Long lines past the former cap are marked
- **WHEN** long-line markers are enabled for a document with 65,536 long lines and the map is scrolled to its end
- **THEN** the strip draws long-line markers on the rows that show those lines
