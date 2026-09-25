# adaptive-editor-geometry Specification

## Purpose
Ensure the editor shell, adaptive secondary surfaces, and persistent chrome settle into stable geometry across compact, narrow, and short-window layouts.
## Requirements
### Requirement: Adaptive secondary surfaces settle from stable layout intent
The system SHALL derive the main window's workspace-sidebar and document-properties presentation from stable layout intent: current window width, selected workspace width preset, explicit requested workspace visibility, explicit requested document-properties visibility, focus-mode suppression, and fixed editor-content minimums. The document-properties pane-or-sheet decision MUST NOT depend on temporary rendered workspace visibility caused by compact mutual exclusion, overlay animation, or a previous pass through the same allocation. Applying the derived state MUST be idempotent while those inputs remain unchanged.

#### Scenario: Medium width with both surfaces requested settles once
- **WHEN** the workspace sidebar and document properties are both explicitly requested open
- **AND** the selected workspace width preset is `Comfy`
- **AND** the window width is greater than the no-workspace document-properties guard and less than or equal to the default workspace-aware document-properties guard
- **THEN** document properties render in the compact bottom-sheet presentation
- **AND** the workspace sidebar is temporarily suppressed as the inactive compact secondary surface
- **AND** the shell does not alternate between right-pane and bottom-sheet presentations while the width and requested visibility inputs remain unchanged

#### Scenario: Spacious width restores both requested surfaces
- **WHEN** the workspace sidebar and document properties are both explicitly requested open
- **AND** the window becomes wide enough for the workspace sidebar, editor content, and document-properties pane to consume layout width together
- **THEN** the workspace sidebar renders as a consuming side surface
- **AND** document properties render as a right-side pane
- **AND** the transition does not discard either requested visibility state

#### Scenario: Settled adaptive state stays quiet
- **WHEN** the shell has applied the derived adaptive state for the current window width and requested surface inputs
- **THEN** the workspace split view, properties split view, properties layout, and bottom sheet do not continue emitting state changes caused by the same allocation
- **AND** another state change occurs only after a user action, focus-mode change, preset change, or real window-size change updates the inputs

### Requirement: Persistent bottom chrome remains allocated at supported short heights
The system SHALL preserve the normal-mode bottom status bar and quick editor-state controls at every supported interactive window height. Fixed chrome, the tab strip, the status bar, and a minimal editor viewport MUST fit within the advertised normal-mode minimum height. Optional surfaces such as search results, workspace content, document properties, and editor overlays MUST yield space before persistent bottom chrome is clipped. Focus Mode MAY suppress the status bar according to its existing focused-writing contract.

#### Scenario: Normal minimum height keeps the status bar visible
- **WHEN** the user resizes the main window to the normal-mode minimum supported height
- **THEN** the status bar has a nonzero visible allocation
- **AND** the editor still has a usable viewport
- **AND** GTK and Libadwaita do not report that the root window content exceeds the allocated height

#### Scenario: Search results yield to persistent chrome
- **WHEN** the search panel is open in a short normal-mode window
- **AND** the available height cannot satisfy the search results' comfortable height together with persistent chrome
- **THEN** the search results area shrinks or collapses within the remaining content budget
- **AND** the status bar remains visible and usable

#### Scenario: Optional side surfaces do not force bottom clipping
- **WHEN** the workspace sidebar, document properties, minimap, or editor inline overlays are visible in a short normal-mode window
- **THEN** those optional surfaces do not cause the status bar to disappear below the bottom allocation
- **AND** any necessary truncation, scrolling, or compacting happens inside the optional surface or editor content area

### Requirement: Narrow workspace layouts preserve the editor left edge
The system SHALL keep the active editor's gutter and line starts visible after passive narrow-width transitions. Crossing the workspace split-view collapse threshold MUST NOT leave the workspace sidebar covering the editor's left edge unless the user explicitly opens the compact workspace overlay. Layout-induced horizontal adjustment changes MUST be clamped so they do not masquerade as intentional user horizontal scrolling.

#### Scenario: Passive shrink does not obscure line starts
- **WHEN** the workspace sidebar is requested open in a side-by-side layout
- **AND** the user passively narrows the window past the workspace collapse threshold without activating the sidebar toggle
- **THEN** the editor gutter and beginning of visible text lines remain visible
- **AND** the workspace sidebar does not remain as an unintended overlay covering the editor content

#### Scenario: Explicit compact workspace overlay is distinguishable
- **WHEN** the window is compact and the user explicitly opens the workspace sidebar
- **THEN** any overlay presentation is treated as the active compact secondary surface
- **AND** the user can dismiss or replace that overlay through the existing workspace and document-properties controls
- **AND** the shell does not persist an overlay-obscured editor state as the result of a passive resize alone

#### Scenario: Passive resize does not preserve stale rightward scroll
- **WHEN** the active editor has no explicit user horizontal-scroll intent
- **AND** a passive layout change reduces the editor viewport width
- **THEN** the horizontal adjustment is clamped to the left edge after layout settles
- **AND** the gutter and line starts remain visible even for long-line documents

### Requirement: Shell width budgets match actual widget minima
The system SHALL base split-view fractions, adaptive guards, and side-surface width reservations on the same minimum sizes advertised by the rendered GTK widgets. Shell constants MUST NOT under-budget a surface relative to its template width request or measured minimum. Guard calculations that include the workspace sidebar MUST use the effective clamped workspace width for the relevant requested layout rather than a stale rendered state from compact suppression.

#### Scenario: Properties pane guard budgets the rendered properties surface
- **WHEN** document properties are eligible for right-pane presentation
- **THEN** the right-pane guard reserves at least the document-properties panel's actual minimum width
- **AND** the resulting editor content allocation does not rely on a smaller hard-coded width than the widget advertises

#### Scenario: Workspace preset changes recompute stable guards
- **WHEN** the user changes the workspace sidebar width preset while document properties are requested open
- **THEN** the adaptive guard is recomputed from the selected preset's effective clamped workspace width
- **AND** the shell settles to the correct pane or sheet presentation without oscillating through rendered sidebar visibility

#### Scenario: Guard boundary captures are warning-free
- **WHEN** the window is captured at widths immediately below, at, and above the no-workspace and workspace-aware document-properties guards
- **THEN** the shell presents a stable secondary-surface state at each width
- **AND** GTK and Libadwaita do not report allocation warnings caused by mismatched shell budgets

### Requirement: Shell transitions preserve editor visual anchors
Adaptive shell transitions SHALL preserve the active editor's top and left visual anchors unless the user has explicitly scrolled away from those anchors. Width-only layout changes, workspace sidebar visibility changes, document-properties pane/sheet changes, compact secondary-surface arbitration, and maximization-like allocation changes MUST NOT create stale scroll adjustments that clip line starts or top content.

#### Scenario: Width-only sidebar transition preserves top and left anchors
- **WHEN** the active editor is scrolled to the top-left origin
- **AND** the workspace sidebar is shown or hidden without changing editor height
- **THEN** the editor remains anchored to the top-left origin after layout settles
- **AND** the minimap top content and viewport overlay use the refreshed editor geometry

#### Scenario: Properties transition does not disturb editor top anchor
- **WHEN** the active editor is scrolled to the top of the document
- **AND** document properties switch between hidden, right-pane, or bottom-sheet presentations
- **THEN** the editor's top visible line remains anchored unless the properties surface intentionally consumes vertical viewport space
- **AND** any intended vertical viewport change is represented in visual geometry state

#### Scenario: Explicit user scroll is respected
- **WHEN** the user has intentionally scrolled horizontally or vertically away from an anchor
- **THEN** shell transition clamping does not force the editor back to the origin
- **AND** the resulting scroll position remains internally consistent with the new adjustment range

### Requirement: Adaptive geometry exposes settled visual state for smoke proof
The adaptive shell SHALL expose enough bounded settled state for smoke helpers to determine whether sidebar, properties, bottom sheet, preview, search panel, status bar, tab strip, and editor content allocations are ready for visual comparison.

#### Scenario: Readiness waits for adaptive layout work
- **WHEN** a visual smoke scenario toggles workspace sidebar or document properties
- **THEN** the visual geometry readiness predicate waits until split-view state, compact-surface state, relevant animations, editor allocation refresh, minimap refresh, and status-bar allocation have settled
- **AND** a timeout reports the first blocker rather than falling back to a blind sleep

#### Scenario: Settled state includes surface rectangles
- **WHEN** Automation1 visual geometry state is requested after adaptive layout settles
- **THEN** it includes bounded rectangles and visibility state for workspace sidebar, document properties, bottom sheet, tab strip, editor viewport, minimap, and status bar when present
- **AND** absent surfaces are represented as intentionally hidden, not omitted ambiguously

### Requirement: Markdown preview uses Adwaita-native presentation
The Markdown preview shell SHALL present editor-only, side-by-side preview, and
preview-only mode through Adwaita-native layout containers instead of an
app-owned `GtkPaned` divider animation. The side-by-side preview MUST be an
explicitly requested end secondary surface for the editor content, and
preview-only mode MUST render the Markdown preview as the focused content area.
The implementation MUST NOT rely on manually animating paned positions,
temporarily changing paned shrink flags, or exposing zero-width paned states to
make preview transitions work.

#### Scenario: Side-by-side preview opens as an end secondary surface
- **WHEN** an active Markdown tab is open in normal editing mode
- **AND** the side-by-side preview target-state action requests visibility
- **THEN** the rendered Markdown preview appears as an end secondary surface for the editor content
- **AND** the editor tab view remains present as the primary content surface
- **AND** no preview-specific `GtkPaned` position animation or shrink-child toggle is required for the state to settle

#### Scenario: Preview-only mode fills the content area
- **WHEN** an active Markdown tab is open
- **AND** the user activates Markdown preview-only mode through `Alt+P`, the primary menu, or the target-state action
- **THEN** the rendered Markdown preview fills the editor content area
- **AND** the side-by-side preview requested state is cleared
- **AND** normal header, tab, status, workspace, and document-properties behavior follows the existing shell contracts for that mode

#### Scenario: Compact side-by-side preview remains explicit
- **WHEN** the window is compact enough that the preview secondary surface cannot consume side-by-side width comfortably
- **AND** the user or automation explicitly requests side-by-side preview
- **THEN** any collapsed or overlay presentation is treated as the active requested preview surface
- **AND** passive window resizing alone does not persist an overlay-obscured editor state
- **AND** persistent chrome, reachable preview dismissal, and the editor left edge remain governed by the adaptive shell contracts

#### Scenario: Preview geometry settles for visual proof
- **WHEN** a visual or widget scenario toggles editor-only, side-by-side preview, or preview-only mode with workspace sidebar, document properties, compact width, or short height states present
- **THEN** readiness waits until the preview layout, relevant Adwaita split or layout-view state, editor allocation refresh, and Markdown embedded widget layout repair have settled
- **AND** the boundary state is free of unexpected GTK, Libadwaita, GDK, renderer, and accessibility warnings

### Requirement: Adaptive geometry remains warning-free at visual invariant boundaries
Adaptive shell geometry SHALL remain free of unexpected GTK, Libadwaita, GDK, renderer, and accessibility warnings at visual invariant boundary states.

#### Scenario: Boundary captures fail on geometry warnings
- **WHEN** visual smoke captures widths immediately below, at, and above workspace or properties layout boundaries
- **THEN** unexpected GTK or Libadwaita allocation warnings fail the scenario
- **AND** the warning scan preserves logs with the matching screenshot and geometry state

### Requirement: Shell animations do not expose stale toolkit-owned editor effects
Adaptive shell transitions SHALL avoid presenting mapped editor or minimap
widgets at geometry epochs where toolkit-owned rendered effects are known to be
stale. When a workspace-sidebar animation would repeatedly reallocate the active
editor width while a native minimap is visible, each presented frame MUST either
use a synchronized editor/minimap allocation for that frame or use a
native-pixel freeze plus settle-once repair that prevents stale
intermediate-width native geometry from being visible. This requirement MUST
NOT be satisfied by replacing, restyling, recoloring, or drawing over the native
minimap highlight.

#### Scenario: Sidebar show stages editor width safely
- **WHEN** the workspace sidebar is requested open from a settled hidden state
- **AND** the active editor has a visible native minimap whose rendered effect depends on editor or source-map width
- **THEN** the shell transition does not present a frame where the editor/minimap allocation has advanced but the native minimap highlight still paints from stale slider geometry
- **AND** the transition ends with the requested sidebar visible and the editor/minimap using final settled geometry

#### Scenario: Sidebar hide stages editor width safely
- **WHEN** the workspace sidebar is requested hidden from a settled visible state
- **AND** the active editor has a visible native minimap whose rendered effect depends on editor or source-map width
- **THEN** the shell transition does not present a frame where the editor/minimap allocation has advanced but the native minimap highlight still paints from stale slider geometry
- **AND** the transition ends with the requested sidebar hidden and the editor/minimap using final settled geometry

#### Scenario: Unsupported same-frame sync uses native-pixel freeze instead of visual replacement
- **WHEN** public GTK or GtkSourceView APIs cannot prove that the native source-map highlight is synchronized before the next painted animation frame
- **THEN** the editor may temporarily present a snapshot of the previous native map pixels while the width-reflow burst is active
- **AND** the product does not switch to an app-owned minimap highlight, re-skin the native highlight, or leave the freeze visible after the settled repair

#### Scenario: Layout containment preserves requested visibility intent
- **WHEN** the shell stages a sidebar transition to protect native minimap rendering
- **THEN** workspace-sidebar requested visibility, compact secondary-surface arbitration, document-properties presentation, focus mode suppression, and saved visibility preferences remain governed by the existing adaptive layout intent
- **AND** only the presentation timing of the transition is affected

### Requirement: Reusable clipping and render-hold preserve shell contracts
After the Phase 3 migration, reusable GTK Lush widget abstractions SHALL
preserve LushText's existing adaptive editor geometry contracts. `ClipBin`
MUST keep flexible content yielding before persistent chrome, and
`RenderHoldOverlay` MUST prevent stale toolkit-rendered minimap frames without
changing requested sidebar/document-properties visibility, focus-mode
suppression, compact-surface arbitration, or final settled layout intent.

#### Scenario: ClipBin preserves persistent bottom chrome
- **WHEN** the main editor shell is constrained to the normal-mode minimum
  supported height after migrating from `LushtextShrinkableBin` to `ClipBin`
- **THEN** the status bar, tab strip, and fixed chrome retain nonzero visible
  allocations
- **AND** optional content yields, clips, or scrolls in its own region instead
  of pushing persistent chrome outside the window

#### Scenario: ClipBin does not add root scrolling
- **WHEN** the shell contains open side surfaces, search results, minimap,
  inline alerts, and awkward editor content inside constrained geometry
- **THEN** no unintended root-level scrollbar is introduced by the reusable
  clipping wrapper
- **AND** GTK and Libadwaita allocation warnings remain absent

#### Scenario: Render hold preserves requested layout intent
- **WHEN** a workspace-sidebar show or hide animation is staged while the
  native minimap is visible
- **THEN** `RenderHoldOverlay` may temporarily present captured native pixels
  for the minimap child
- **AND** workspace requested visibility, document-properties requested
  visibility, compact secondary-surface arbitration, focus mode, and saved
  preferences remain governed by the existing adaptive layout state

#### Scenario: Render hold ends with synchronized live widgets
- **WHEN** a render hold is cleared after the reflow settle repair or revealed
  early because of user interaction
- **THEN** the live editor and minimap widgets are visible, synchronized with
  the final layout, and warning-free
- **AND** no stale cover remains mapped over the final shell

### Requirement: Adaptive shell policy invariants are machine-checked
The project SHALL keep Kani harnesses over the production adaptive-shell
policy in `ui/window/geometry/policy.rs`. Their domain SHALL be every `i32`
window width, every workspace preset, every combination of requested
visibility and Focus Mode, and every compact-surface choice. The harnesses
SHALL prove that `derive_adaptive_shell_layout`:

- renders no secondary surface while Focus Mode is active;
- never renders a surface that is not requested;
- renders at most one secondary surface in the compact presentation;
- renders every requested surface in the wide presentation, above the
  workspace breakpoint and outside Focus Mode;
- chooses the sheet presentation exactly when the window width is at or below
  its derived breakpoint threshold.

The policy SHALL be whole-pixel: it takes widths and returns widths and pane
shares (a pane width and the width it is a share of) in whole sp, and the
split-view fraction SHALL be formed once, in the GTK adapter. The harnesses
SHALL also prove, for every workspace width from 0 to 440 sp, that
`properties_breakpoint_max_width_sp` never decreases as the width grows, and
that the threshold is at least the editor-content minimum, plus the layout
overhead, the workspace width, and the properties minimum. For every `i32`
width, every pane share SHALL have a positive width and a positive
denominator, and a properties share taken of the inner split SHALL have a
denominator narrower than the window; the adapter's split fraction SHALL be
tested to be finite and in (0, 1]. No function in the module SHALL panic.

#### Scenario: Compact presentation shows one secondary surface
- **WHEN** the harness explores every input for which document properties use the sheet presentation
- **THEN** the workspace sidebar and document properties are never both rendered

#### Scenario: Focus Mode suppresses every surface
- **WHEN** Focus Mode is active, for any width and requested visibility
- **THEN** neither secondary surface is rendered

### Requirement: Side-by-side preview width is bounded by one third of the content width above its floor
The side-by-side Markdown preview SHALL resolve its width from the preferred
width and the available content width. A preferred width of zero or less
resolves to the 300 sp default. An available width of zero or less counts as
1 sp. The resolved width SHALL satisfy all of these:

- it is at least the 1 sp floor;
- when the available width is at least 3 sp, it is at most one third of the
  available width;
- when the available width is below 3 sp, it equals the 1 sp floor;
- when the preferred width lies between the floor and that one-third bound, it
  equals the preferred width.

The resolution SHALL be a GTK-free policy function that takes and returns
whole pixels (`i32`), with no floating-point arithmetic inside it; the window
SHALL convert the result to `f64` once, where it sets the split view's
constraints. Kani harnesses SHALL prove these properties over every `i32`
preferred and available width. A kept
`should_panic` harness SHALL show that the unconditional "at most one third"
form is false below 3 sp.

#### Scenario: Normal widths respect the one-third bound
- **WHEN** the available content width is 900 sp and the preferred width is 500 sp
- **THEN** the preview width is at most 300 sp

#### Scenario: A tiny content width keeps the non-zero floor
- **WHEN** the available content width is 2 sp
- **THEN** the preview width is the 1 sp floor, which exceeds one third of the available width

### Requirement: The adaptive-shell breakpoint loop is verified against the axiom ledger
The project SHALL keep a Kani-checked step model of the adaptive-shell
breakpoint feedback loop: allocated window width → derived layout →
properties breakpoint threshold and Adwaita breakpoint state → rendered
surfaces → allocated window width.

The model SHALL call the real `derive_adaptive_shell_layout` and the real
reconciliation decision the execution module applies. It SHALL model Adwaita
only through `kani::assume` clauses that cite ledger axiom ids.

For every workspace preset, every requested-visibility and compact-slot
combination, Focus Mode on and off, and window widths across the supported
range, the model SHALL prove the following.

- **Fixed point:** with stable inputs, the cached threshold, the installed
  breakpoint condition, the breakpoint's applied state, the properties layout
  name, the compact slot, and the rendered visibility of both surfaces stop
  changing within a stated bound of allocations. One further allocation at the
  same width then changes nothing.
- **No flapping:**
  - with stable inputs, the properties presentation changes at most once;
  - across a monotone width sweep, it changes at most once per threshold
    crossing.
- **Requested visibility is preserved:** no step writes the requested
  workspace or document-properties visibility. Whenever the width admits both
  surfaces, the rendered visibility equals the requested visibility.
- **Agreement:** at rest, the rendered properties layout equals the
  presentation the policy derives.
- **No persistence on the allocation path:** no step reachable from an
  allocation writes a width fraction to settings.

A property that fails SHALL be fixed starting from a failing real-GTK widget
test when real GTK can reach it. When it is not reachable, it SHALL be kept as
a named `#[kani::should_panic]` residual harness, with reachability evidence
in the programme record.

#### Scenario: Stable width settles
- **WHEN** the window width, preset, requested visibility, compact slot and Focus Mode are held fixed for any admissible Adwaita behaviour
- **THEN** the model reaches a state that a further allocation at the same width leaves unchanged, within the stated bound

#### Scenario: A width sweep does not flap
- **WHEN** the window width moves monotonically across the properties breakpoint threshold
- **THEN** the properties presentation changes at most once for that crossing and never alternates between pane and sheet

#### Scenario: Requested visibility survives compact layouts
- **WHEN** a compact width suppresses the workspace sidebar or document properties and the width then widens back to one that admits both
- **THEN** both surfaces render as requested and the requested flags were never written by the loop

### Requirement: The shell reconciliation decision is pure policy
The decision that reconciles the rendered shell surfaces with a derived layout
SHALL be a GTK-free pure function in the workflow's `policy.rs`. It covers:

- which properties layout name to set;
- which compact slot to store;
- which `show-sidebar` and bottom-sheet `open` values to write;
- whether the breakpoint condition must be re-installed.

The execution module SHALL apply the returned writes without re-deciding
them. The extraction SHALL be behaviour-preserving: characterization tests
written before the move SHALL pass unchanged after it.

#### Scenario: The model drives production logic
- **WHEN** the breakpoint-loop harness takes a reconciliation step
- **THEN** it calls the same pure function that `sync_secondary_surfaces` and `sync_properties_breakpoint` apply in production

#### Scenario: Extraction preserves behaviour
- **WHEN** the reconciliation decision moves into `policy.rs`
- **THEN** the existing shell-geometry widget tests and the new characterization tests pass unchanged, and allocation-time sync still performs no settings write

### Requirement: The reconciliation plan is the only writer of the properties layout
The properties breakpoint SHALL NOT carry a setter for the properties layout
name. Its `apply` and `unapply` signals SHALL run the shell reconciliation, so
the pure plan is the only writer of `layout-name`. A setter restores its
add-time value on unapply (ledger A16) and, under text scaling, disagrees with
the policy's px-against-sp comparison (ledger A14), so two writers made one
allocation change the layout twice.

#### Scenario: Large text does not flip the pane
- **WHEN** the text scale is 1.25 and the window is 1500 px wide
- **THEN** the properties layout does not change during an allocation at a fixed width

### Requirement: Every breakpoint that narrows the shell collapses the workspace
Because only the last-added matching breakpoint applies (ledger A18), every
breakpoint whose condition implies the workspace breakpoint's SHALL also set the
workspace split view `collapsed`, so switching between them at a fixed width
never uncollapses the workspace.

#### Scenario: Doubled text scale at the minimum width
- **WHEN** the text scale is 2.0 and the window is 700 px wide, and the text scale then switches between 2.0 and 1.5
- **THEN** the workspace split view stays collapsed throughout

