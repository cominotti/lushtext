# editor-minimap Specification

## Purpose
Provide a toggleable editor minimap that mirrors document state, shows semantic navigation markers, and supports viewport-aware navigation without changing the surrounding window layout.
## Requirements
### Requirement: Users can toggle the editor minimap
The system SHALL provide a persistent minimap preference and window action that show or hide a minimap on eligible editor pages. When enabled, the minimap SHALL appear on the right edge of supported editor pages without changing the outer workspace or properties sidebar visibility model.

#### Scenario: Enable the minimap for a supported document
- **WHEN** the user enables the minimap while the active editor page supports it
- **THEN** the active editor page shows a minimap on its right edge
- **AND** later editor pages in the same enabled state also show the minimap when they support it

#### Scenario: Minimap preference persists across sessions
- **WHEN** the user closes LushText with the minimap enabled and later reopens the app
- **THEN** supported editor pages restore with the minimap visible
- **AND** unsupported pages keep the preference enabled without forcing the minimap to appear

### Requirement: Minimap preferences stay grouped on the Editor page
The system SHALL expose minimap-specific preferences in a dedicated `Minimap` group on the existing `Editor` preferences page. The system MUST NOT add a top-level `Minimap` preferences page while the minimap preference surface contains only the minimap visibility control and the long-line marker visibility control.

#### Scenario: Editor page shows minimap controls together
- **WHEN** the user opens Preferences and views the `Editor` page
- **THEN** the page contains a `Minimap` group
- **AND** that group contains the `Show Minimap` control
- **AND** that group contains the `Show Long-Line Markers` control

#### Scenario: Long-line marker preference defaults off
- **WHEN** LushText starts with default settings
- **THEN** the `Show Long-Line Markers` preference is disabled
- **AND** enabling the minimap does not automatically enable long-line markers

### Requirement: The minimap stays synchronized with the active document
The system SHALL render the minimap from the same active editor buffer and SHALL keep its viewport indication synchronized with document scrolling and editing. The minimap MUST remain read-only and MUST NOT become a second editable text surface.

#### Scenario: Scrolling the editor updates the minimap viewport
- **WHEN** the user scrolls the active editor page
- **THEN** the minimap updates its viewport indicator to the corresponding document region
- **AND** the minimap continues to represent the same document content as the editor

#### Scenario: Editing the document refreshes the minimap
- **WHEN** the user changes the active document text
- **THEN** the minimap refreshes to reflect the updated document shape
- **AND** the user cannot place a text cursor or type into the minimap itself

### Requirement: The minimap shows a clear viewport overlay
The system SHALL render the native `GtkSourceMap` rectangular semi-transparent viewport highlight inside the minimap so users can see which portion of the document is currently visible in the main editor. The highlight SHALL preserve the existing neutral editor-chrome fill, border, sizing, and interaction behavior. LushText MUST NOT replace the visible native highlight with an app-owned duplicate or restyled substitute for this behavior. The highlight SHALL remain visible whenever the minimap itself is visible, MUST NOT use system accent color styling, and MUST remain visually subordinate to semantic markers such as bookmarks, search matches, modified lines, and long-line warnings.

#### Scenario: Viewport overlay tracks the visible region during scrolling
- **WHEN** the user scrolls the active editor page while the minimap is visible
- **THEN** the minimap updates the native viewport highlight position to the corresponding document region
- **AND** the highlight remains visually distinct from semantic markers behind it
- **AND** the highlight does not use system accent color styling

#### Scenario: Viewport overlay fills the minimap when the full document is visible
- **WHEN** the active document fits entirely within the editor viewport and the minimap is enabled
- **THEN** the minimap remains visible
- **AND** the native viewport highlight expands to indicate that the full document is currently visible
- **AND** the expanded highlight remains neutral and visually calm rather than appearing as an accent-colored block

#### Scenario: Native viewport effect is not replaced
- **WHEN** the minimap is visible for a supported document
- **THEN** users see the existing native `GtkSourceMap` viewport highlight effect
- **AND** no second visible viewport overlay duplicates or hides that native effect
- **AND** minimap click and drag navigation continue to use the normal source-map interaction path

### Requirement: Users can navigate from the minimap
The system SHALL let users navigate the active document from the minimap by clicking or dragging inside it. Minimap navigation SHALL move the main editor viewport to the corresponding document position and SHALL keep editor focus behavior consistent with normal document interaction.

#### Scenario: Clicking the minimap jumps to a document region
- **WHEN** the user clicks a position in the minimap
- **THEN** the active editor scrolls to the corresponding document region
- **AND** the editor remains the primary editing surface after the jump

#### Scenario: Dragging inside the minimap scrolls continuously
- **WHEN** the user drags through the minimap
- **THEN** the active editor viewport follows that drag through the document
- **AND** the minimap viewport indicator updates continuously during the drag

### Requirement: The minimap shows semantic navigation markers
The system SHALL render visually distinguishable semantic markers in the minimap for editor-local navigation signals. The default supported marker set SHALL include bookmarks, active in-tab search matches, and modified-since-save line regions. Lines above the minimap warning threshold SHALL be represented by long-line warning markers only when the user enables the long-line marker preference. Semantic marker positions SHALL be projected from the same rendered `GtkSourceMap` geometry used by the minimap content, including top margin, dynamic EOF overscroll, and allocation changes. Semantic markers MUST NOT be spread across blank EOF overscroll tail space when their corresponding document lines render above that tail.

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

### Requirement: Minimap availability adapts to high-cost documents without hiding enabled supported tabs
The system SHALL keep the minimap visible on supported editor pages whenever the user's minimap preference is enabled, including when the full document already fits inside the editor viewport. The system SHALL suppress the per-tab minimap instance only when the active document exceeds the file-size tier that supports minimap rendering or another unsupported editor state prevents a safe minimap presentation. Suppressing the minimap for one document MUST NOT silently disable the user's saved minimap preference for other eligible documents.

#### Scenario: Fully visible document still keeps the minimap visible
- **WHEN** the active document fits entirely inside the current editor viewport and the minimap preference is enabled
- **THEN** the editor page still renders the minimap for that document view
- **AND** the saved minimap preference remains enabled for other supported documents

#### Scenario: Large document keeps the preference but disables the minimap
- **WHEN** the user opens or focuses a document that exceeds the minimap-supported size tier
- **THEN** the editor page does not render the minimap for that document
- **AND** the user receives feedback that the minimap is unavailable for the current document

### Requirement: Large minimap analysis yields and rejects stale generations
For documents within the existing minimap-supported tier, wrapped-layout and long-line analysis SHALL use cached evidence or bounded GTK snapshot or iterator slices. No scheduled turn may inspect more than the configured character or item budget, and every analysis session MUST carry editor lifetime and minimap-analysis generation. Buffer edits, wrap changes, marker preferences, file replacement, or page teardown MUST invalidate stale analysis before another slice or projection is accepted.

#### Scenario: Wrapped many-short-line document exceeds one slice
- **WHEN** the minimap is enabled with wrapping for a supported multi-megabyte document containing many short lines
- **THEN** layout analysis runs over bounded GTK turns rather than scanning the complete buffer in one callback
- **AND** the minimap remains visible while supported analysis reaches its current terminal state

#### Scenario: Edit supersedes active analysis
- **WHEN** the document changes after one or more analysis slices but before terminal publication
- **THEN** the stale generation stops before applying layout or marker results
- **AND** only the latest generation may update minimap availability, cache, or warnings

#### Scenario: Long-line markers reuse bounded current evidence
- **WHEN** optional long-line warnings and wrapped-layout availability require overlapping document analysis
- **THEN** they reuse current cached or sliced evidence instead of performing separate full-buffer scans or copies
- **AND** disabling markers releases marker-only state without invalidating unrelated minimap features

#### Scenario: Unsupported size tier remains explicit
- **WHEN** a document exceeds the existing minimap-supported file-size tier
- **THEN** the editor keeps the saved minimap preference and shows the existing unavailable feedback
- **AND** the bounded-analysis workflow does not introduce a new lower byte-only hide threshold

### Requirement: Focus Mode temporarily hides the minimap
The system SHALL suppress editor minimap rendering while Focus Mode is active, regardless of the user's saved minimap preference. Focus Mode MUST NOT change the saved minimap preference, and normal minimap availability MUST resume when Focus Mode exits.

#### Scenario: Enabled minimap hides while focused
- **WHEN** the user's minimap preference is enabled and a supported editor page shows the minimap
- **AND** the user enters Focus Mode
- **THEN** the minimap is hidden
- **AND** the saved minimap preference remains enabled

#### Scenario: Minimap restores after focus
- **WHEN** Focus Mode is active after hiding an enabled minimap
- **AND** the user exits Focus Mode
- **THEN** the minimap renders again for supported editor pages
- **AND** unsupported editor pages continue to follow normal minimap availability rules

#### Scenario: Disabled minimap remains disabled after focus
- **WHEN** the user's minimap preference is disabled
- **AND** the user enters and exits Focus Mode
- **THEN** the minimap remains disabled

### Requirement: Minimap viewport follows settled editor geometry after width changes
The system SHALL keep the minimap viewport overlay synchronized with the active editor's settled visible buffer range after layout-driven width changes. Sidebar show/hide, compact surface arbitration, editor width-only allocation changes, word-wrap reflow, and end-of-file overscroll recalculation MUST NOT leave the minimap viewport overlay using stale pre-transition geometry. The minimap's wrapping and viewport projection policy MUST be explicit enough that the overlay corresponds to the main editor viewport even when the minimap content uses a different wrap mode from the editor view.

#### Scenario: Sidebar toggle preserves viewport correspondence
- **WHEN** the minimap is enabled for the active document
- **AND** word wrap is enabled
- **AND** the workspace sidebar is shown or hidden at a width where it changes the editor allocation
- **THEN** after layout settles, the minimap viewport overlay corresponds to the active editor's visible buffer range
- **AND** the overlay is not positioned from stale pre-toggle adjustment geometry

#### Scenario: Width-only allocation refreshes the minimap viewport
- **WHEN** the active editor's width changes without a corresponding editor height change
- **AND** that width change can alter wrapping, visible-line geometry, or scroll adjustment ranges
- **THEN** the minimap viewport overlay is refreshed after the new editor and source-map allocations settle
- **AND** semantic minimap markers continue to use the refreshed source-map geometry

#### Scenario: Wrapping policy does not create viewport drift
- **WHEN** the main editor and minimap use different wrap modes
- **AND** the window width changes enough to reflow wrapped editor lines
- **THEN** the minimap viewport overlay still represents the main editor's settled visible buffer range under the chosen projection policy
- **AND** the user does not see the viewport indicator jump solely because the minimap retained a stale logical-to-visual mapping

#### Scenario: Word-wrap-disabled control remains stable
- **WHEN** word wrap is disabled for the active editor
- **AND** the workspace sidebar is shown or hidden while the minimap is visible
- **THEN** the minimap viewport overlay remains aligned with the active editor's visible buffer range after layout settles
- **AND** any residual viewport drift is not hidden by word-wrap-specific assumptions

### Requirement: Minimap top content remains visible after shell reflow
The minimap SHALL keep the first rendered minimap content visible and unclipped when the active editor is at the top of the document. Width-only shell reflow, workspace sidebar show/hide, document-properties presentation changes, word-wrap changes, style changes, and dynamic overscroll refreshes MUST NOT leave the minimap's top content hidden under its border or clipped by stale source-map geometry.

#### Scenario: Sidebar hide keeps minimap top line visible
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is hidden at a width where the editor allocation changes
- **THEN** the minimap's first rendered content line remains visible below the minimap's top edge
- **AND** the main editor's first visible line remains line one

#### Scenario: Sidebar show keeps minimap top line visible
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is shown at a width where the editor allocation changes
- **THEN** the minimap's first rendered content line remains visible below the minimap's top edge
- **AND** the minimap viewport overlay corresponds to the main editor's top visible range after layout settles

#### Scenario: Width-only reflow refreshes top anchoring
- **WHEN** the active editor's width changes without a corresponding height change
- **AND** the editor was already vertically anchored at the top
- **THEN** the editor and minimap refresh top anchoring after the new allocation settles
- **AND** no stale vertical adjustment or source-map projection clips the minimap's top content

#### Scenario: Theme and wrap controls do not hide top content
- **WHEN** the minimap is visible at the top of the document
- **AND** the user switches between light and dark style preference or enables/disables word wrap
- **THEN** the minimap top content remains visible and unclipped after the style and layout settle
- **AND** semantic markers continue to project from the refreshed source-map geometry

### Requirement: Minimap geometry exposes bounded visual anchors
The minimap SHALL provide bounded geometry anchors for automation and visual smoke without exposing document contents. Anchors MUST include enough information to prove minimap shell bounds, source-map bounds, top content inset, viewport overlay bounds, marker-strip bounds, and scroll/top state.

#### Scenario: Automation can identify minimap bounds
- **WHEN** Automation1 visual geometry state is requested while the minimap is visible
- **THEN** it reports bounded minimap shell, source-map, viewport-overlay, and marker-strip rectangles in the documented coordinate space
- **AND** it does not expose minimap-rendered text or document body content

#### Scenario: Hidden minimap reports absence explicitly
- **WHEN** Automation1 visual geometry state is requested while the minimap is disabled, too large, evicted, or focus-suppressed
- **THEN** the minimap geometry entry records the unavailable state
- **AND** visual smoke can distinguish intentional absence from missing geometry data

### Requirement: Minimap widget tests cover logical top-edge invariants
The project SHALL provide widget-level regression coverage for minimap top anchoring and width reflow. These tests SHALL assert logical geometry, allocation, scroll adjustment, and marker projection state without depending on compositor pixels.

#### Scenario: Top scroll stays anchored across sidebar toggle
- **WHEN** a widget test creates a real window with minimap enabled and the active editor at the top of a long document
- **AND** it toggles workspace sidebar visibility
- **THEN** the source view vertical adjustment remains at its lower bound after layout settles
- **AND** the minimap source-map allocation remains positive and refreshed

#### Scenario: Mid-file reflow does not jump document range
- **WHEN** a widget test scrolls a minimap-enabled editor to a representative middle line
- **AND** shell reflow changes the editor width
- **THEN** the visible start line remains within the existing accepted tolerance
- **AND** minimap marker bounds remain projectable through the source-map geometry

### Requirement: Native minimap highlight remains stable during sidebar animation frames
The native minimap viewport highlight SHALL remain rendered-pixel stable while
workspace-sidebar show and hide animations are in progress. The implementation
MUST preserve the existing native `GtkSourceMap` highlight effect, styling,
interaction behavior, marker layering, and final settled geometry. The
implementation MUST NOT replace the highlight with an app-owned drawing, restyle
or recolor the highlight, or treat final settled correctness as sufficient when
sampled animation frames show drift. During a detected width-reflow burst,
LushText MAY use `gtk-lush-widgets::RenderHoldOverlay` or a documented
compatibility adapter to show a snapshot of the last already rendered native map
pixels, provided the hold is removed after the settle repair or early user
reveal, restores the live source map on every exit path, and never introduces a
new highlight appearance.

#### Scenario: Sidebar show preserves native highlight rows during animation
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** word wrap and the current window width make the workspace-sidebar show animation change editor width
- **THEN** every sampled animation frame keeps the native minimap viewport top edge within the declared row tolerance
- **AND** sampled frames keep the first rendered minimap content row within the declared tolerance
- **AND** the final settled frame still satisfies the existing minimap top-content and viewport-overlay requirements

#### Scenario: Sidebar hide preserves native highlight rows during animation
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** word wrap and the current window width make the workspace-sidebar hide animation change editor width
- **THEN** every sampled animation frame keeps the native minimap viewport top edge within the declared row tolerance
- **AND** sampled frames keep the first rendered minimap content row within the declared tolerance
- **AND** the final settled frame still satisfies the existing minimap top-content and viewport-overlay requirements

#### Scenario: App geometry cannot excuse rendered native drift
- **WHEN** Automation1 reports stable or expected minimap geometry during a sidebar animation
- **AND** screenshot-derived pixel anchors show the native minimap highlight or another scenario-declared anchor drifting outside tolerance
- **THEN** the minimap animation invariant fails
- **AND** the failure preserves bounded app-vs-rendered diagnostics for review

#### Scenario: Native effect remains unchanged
- **WHEN** the animation-frame minimap fix is active
- **THEN** the minimap continues to use the native `GtkSourceMap` viewport highlight for visible presentation
- **AND** any temporary freeze during width reflow is a copy of the native rendered map pixels rather than a replacement drawing
- **AND** minimap navigation, read-only behavior, marker layering, focus behavior, and final settled appearance remain unchanged from the existing native effect

#### Scenario: Animation stability does not depend on marker recomputation
- **WHEN** semantic minimap markers are present while the workspace sidebar animates
- **THEN** lightweight native source-map geometry stays synchronized for the rendered viewport highlight during sampled frames
- **AND** expensive semantic marker recomputation MAY remain debounced if markers settle correctly and do not obscure or contradict the native highlight contract

#### Scenario: Render hold restores the live source map
- **WHEN** a native minimap render hold is captured, warmed, revealed,
  superseded, cancelled, or dropped because the editor tab closes
- **THEN** the live source map opacity and visibility are restored
- **AND** no stale captured cover remains visible over the minimap
- **AND** automation-visible minimap state can distinguish an intentional
  in-progress hold from a stuck invisible source map

#### Scenario: User scroll reveals held minimap promptly
- **WHEN** the user scrolls, drags, or clicks the minimap or editor while a
  render hold is waiting for the post-settle reveal
- **THEN** the hold is revealed or cleared promptly
- **AND** the live `GtkSourceMap` handles navigation and viewport updates
  through its normal path

### Requirement: Native minimap highlight remains rendered-pixel stable after width reflow
The minimap SHALL preserve the existing native `GtkSourceMap` viewport highlight effect after sidebar visibility changes, width-only editor reallocations, word-wrap reflow, dynamic overscroll refreshes, and top-of-document anchoring. The system MUST keep the native highlight's rendered top edge, fill, border, neutral styling, interaction behavior, and marker layering; it MUST NOT satisfy this requirement by replacing the native highlight with an app-owned visible overlay. After layout and native source-map frame work settle, screenshot-derived native-highlight anchors SHALL remain stable according to the visual invariant manifest.

#### Scenario: Sidebar hide preserves rendered top anchors at reproduced size
- **WHEN** the minimap is enabled for a supported document at the top of the file
- **AND** word wrap is enabled
- **AND** the window uses the reproduced intermediate geometry around `1822x1272`
- **AND** the workspace sidebar is hidden from its fully shown state
- **THEN** after final sidebar, editor, and minimap allocation settles, the screenshot-derived native viewport top edge remains at the same window-relative y position
- **AND** the screenshot-derived first rendered minimap content row remains at the same window-relative y position
- **AND** app-computed minimap geometry alone cannot satisfy the scenario if the rendered pixels drift

#### Scenario: Sidebar show preserves rendered top anchors at reproduced size
- **WHEN** the minimap is enabled for a supported document at the top of the file
- **AND** word wrap is enabled
- **AND** the window uses the reproduced intermediate geometry around `1822x1272`
- **AND** the workspace sidebar is shown from its fully hidden state
- **THEN** after final sidebar, editor, and minimap allocation settles, the screenshot-derived native viewport top edge remains at the same window-relative y position
- **AND** the screenshot-derived first rendered minimap content row remains at the same window-relative y position
- **AND** the native highlight remains the same neutral `GtkSourceMap` effect rather than a replacement overlay

#### Scenario: Conventional sizes remain controls, not substitutes
- **WHEN** the minimap/sidebar visual matrix runs at conventional sizes such as 720p, 1080p, 1440p, or `1600x1000`
- **THEN** those cases verify the same native rendered-highlight behavior for their geometry
- **AND** passing those sizes does not remove the requirement to run the reproduced intermediate-size case

#### Scenario: Wrap and theme controls preserve the native effect
- **WHEN** the minimap is visible and the sidebar is shown or hidden after layout settles
- **AND** the scenario uses dark theme, light theme, word-wrap enabled, or word-wrap disabled controls
- **THEN** the native viewport highlight remains rendered-pixel stable for that state
- **AND** semantic minimap markers remain visible above or beside the native highlight according to their existing layering contract

### Requirement: Native minimap diagnostics explain rendered slider position
The minimap SHALL expose bounded diagnostic geometry that can explain the native `GtkSourceMap` slider position without becoming the pass/fail oracle for rendered pixels. Diagnostics SHALL include an upstream-informed native-slider estimate derived from public text-view geometry, the map's own visible rect or adjustment state, final allocation, and app-vs-rendered comparison results when screenshot anchors are available.

#### Scenario: Diagnostic estimate includes map visible state
- **WHEN** Automation1 visual geometry is requested while the minimap is visible
- **THEN** the minimap diagnostics include bounded source-map allocation, editor visible-rect summary, map visible-rect or adjustment summary, native-slider estimate, and existing line-projection anchors
- **AND** the diagnostics do not expose document text or minimap-rendered text

#### Scenario: Rendered disagreement is diagnostic failure evidence
- **WHEN** app-computed native-slider diagnostics report a stable y position
- **AND** screenshot-derived native-highlight pixels move outside the declared tolerance
- **THEN** the visual artifact records an app-vs-rendered disagreement
- **AND** the product invariant fails until the rendered pixels are stable

### Requirement: Native minimap animation sync stays responsive
The minimap SHALL keep animation-frame source-map synchronization lightweight
enough that sidebar animation and editor interaction remain responsive. Any work
performed on the frame path MUST avoid full document scans, unbounded text
snapshots, synchronous filesystem work, or repeated marker rebuilds.

#### Scenario: Frame-path sync avoids expensive document work
- **WHEN** a sidebar animation produces repeated editor width allocations
- **THEN** the minimap frame path synchronizes only bounded native source-map geometry and adjustment state
- **AND** long-line scans, search marker collection, bookmark marker rebuilds, and modified-line marker rebuilds remain debounced or otherwise bounded

#### Scenario: Rapid sidebar toggles do not accumulate stale frame callbacks
- **WHEN** the user toggles the workspace sidebar repeatedly before a previous animation fully settles
- **THEN** stale minimap animation-frame callbacks are ignored by generation or visibility checks
- **AND** the final native highlight and editor scroll state remain correct

### Requirement: Native minimap viewport edge keeps its rendered content anchor
The minimap SHALL keep the rendered top edge of the native `GtkSourceMap` viewport highlight stable relative to the first rendered minimap content row when the active editor is anchored at the top of the document. Sidebar show/hide, width-only reflow, word-wrap changes, style changes, and dynamic overscroll refreshes MUST NOT remove, clip, or shift that native top-edge treatment by more than the declared pixel tolerance for the current visual scenario.

#### Scenario: Sidebar show preserves native viewport edge
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is shown at a width where the editor allocation changes
- **THEN** the native minimap viewport highlight top edge remains rendered
- **AND** the first minimap content row remains rendered
- **AND** the vertical delta between those rendered screenshot anchors remains within the declared tolerance

#### Scenario: Sidebar hide preserves native viewport edge
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is hidden at a width where the editor allocation changes
- **THEN** the native minimap viewport highlight top edge remains rendered
- **AND** the first minimap content row remains rendered
- **AND** the vertical delta between those rendered screenshot anchors remains within the declared tolerance

#### Scenario: Theme and wrap controls preserve native edge identity
- **WHEN** the minimap is visible at the top of the document
- **AND** the visual scenario runs in light and dark style preferences with word wrap enabled and disabled
- **THEN** each run finds the native viewport top-edge anchor using the style-appropriate screenshot detector
- **AND** no run accepts viewport fill pixels, minimap background pixels, or semantic marker pixels as a replacement for the top-edge anchor

#### Scenario: Mid-file native highlight remains synchronized
- **WHEN** the minimap is visible and the editor is scrolled to a representative middle document range
- **AND** shell reflow changes the editor width
- **THEN** the native viewport highlight remains synchronized with the settled visible editor range
- **AND** the highlight bounds remain projectable through the same source-map geometry as semantic minimap markers

### Requirement: Wrapped-layout admission uses conservative live bytes
Minimap wrapped-layout analysis SHALL use the existing O(1) conservative live-buffer estimate: the greater of known file bytes and character count multiplied by four with saturating arithmetic. It MUST NOT scan or copy buffer text merely to classify size, the exact 2 MiB budget MUST remain eligible, and an estimate one byte above the budget MUST enter bounded long-line analysis when wrapping is enabled.

#### Scenario: Multibyte or untitled content exceeds the threshold
- **WHEN** a modified or untitled buffer has no sufficiently large file-size floor but its character count multiplied by four exceeds 2 MiB
- **THEN** wrapped-layout admission starts bounded long-line analysis
- **AND** it does not treat Unicode scalar count as byte count

#### Scenario: Known file size is the conservative floor
- **WHEN** known file bytes exceed the character-derived estimate
- **THEN** the known file size controls wrapped-layout admission
- **AND** the calculation remains O(1)

#### Scenario: Estimate is exactly at the threshold
- **WHEN** the conservative estimate equals the 2 MiB budget
- **THEN** the ordinary eligible path remains available
- **AND** only an estimate above the budget triggers the large-buffer analysis policy

#### Scenario: Arithmetic would overflow
- **WHEN** the character-count estimate cannot be multiplied by four without overflow
- **THEN** saturating arithmetic classifies it conservatively as large
- **AND** no text scan or allocation is introduced

#### Scenario: Wrapping is disabled or generation becomes stale
- **WHEN** wrapping is disabled, the editor is evicted, or the minimap generation changes
- **THEN** existing disabled, eviction, cancellation, and stale-result behavior remains in force
- **AND** no obsolete analysis result changes the minimap
