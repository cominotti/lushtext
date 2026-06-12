# gtk-lush-widgets Specification

## Purpose
Define the reusable GTK Lush geometry widgets used by stock gtk-rs
applications and by LushText adaptive editor/minimap workflows.

## Requirements
### Requirement: Leaf geometry widget crate
`gtk-lush-widgets` SHALL provide small GTK geometry widgets for stock gtk-rs
applications while remaining an independently adoptable GTK Lush leaf crate.
The crate MUST NOT depend on LushText crates, MUST NOT depend on another GTK
Lush family crate at runtime, MUST NOT replace Libadwaita adaptive containers,
and MUST NOT introduce a view DSL, component system, or app shell framework.

#### Scenario: Standalone application adopts only widgets
- **WHEN** `cargo test -p gtk-lush-widgets --examples` builds the crate's
  standalone example
- **THEN** the example uses stock gtk-rs plus `gtk-lush-widgets`
- **AND** no other GTK Lush crate or LushText crate is required

#### Scenario: Runtime family dependency is rejected
- **WHEN** `gtk-lush-widgets` declares another `gtk-lush-*` crate as a non-dev
  dependency
- **THEN** the family policy check fails until the dependency is removed

#### Scenario: Libadwaita remains authoritative
- **WHEN** a consumer uses a GTK Lush geometry widget inside an Adwaita layout
- **THEN** Libadwaita still owns adaptive breakpoints, split views, sheets, and
  animations
- **AND** the widget only enforces its local geometry contract

### Requirement: ClipBin yields minimum size and clips its child
`ClipBin` SHALL be a single-child GTK widget that reports zero minimum size,
delegates natural size and natural baseline to its child, allocates the child
inside the available allocation, and clips child snapshots to the bin's
allocation. It MUST expose an ordinary `child` property usable from
Blueprint/GtkBuilder and ordinary gtk-rs code.

#### Scenario: Empty ClipBin measures empty
- **WHEN** a `ClipBin` has no child or a child that should not layout
- **THEN** it reports zero minimum and zero natural contribution where GTK
  expects an empty widget
- **AND** snapshot and allocation are no-ops without warnings

#### Scenario: Flexible content yields before chrome
- **WHEN** a child has a large minimum or natural height inside a constrained
  window
- **THEN** `ClipBin` reports zero minimum for the bin itself
- **AND** persistent chrome outside the flexible content can remain allocated
  according to the surrounding layout

#### Scenario: Snapshot is clipped
- **WHEN** the child paints outside the `ClipBin` allocation
- **THEN** the rendered output is clipped to the bin bounds
- **AND** no child pixels are allowed to cover adjacent fixed chrome

#### Scenario: Builder child replacement is safe
- **WHEN** the `child` property is set, replaced, cleared, or set to the same
  widget again
- **THEN** old children are unparented exactly once
- **AND** duplicate replacements do not trigger unnecessary reparenting,
  resize loops, or GTK warnings

### Requirement: RenderHoldOverlay captures and restores live widget pixels
`RenderHoldOverlay` SHALL provide a reusable render-hold owner for one live GTK
child and one non-interactive cover. A hold MUST capture the child's currently
rendered pixels when the child is mapped, drawable, and has positive bounds;
show those pixels above the child; hide the live child only through a paired
opacity change or documented equivalent; and restore the live child on every
clear, reveal, dispose, failed capture, or early-exit path.

#### Scenario: Successful hold shows captured native pixels
- **WHEN** the live child is mapped, drawable, has positive bounds, and a
  renderer can produce a texture from `snapshot_child`
- **THEN** the overlay displays a non-interactive cover containing the captured
  child pixels
- **AND** the live child is hidden only while the cover is visible

#### Scenario: Failed capture leaves child visible
- **WHEN** the live child is unmapped, undrawable, empty, missing a renderer,
  or otherwise cannot be captured
- **THEN** the hold request fails without showing a stale cover
- **AND** the live child remains fully visible and interactive according to its
  original state

#### Scenario: Cleanup restores opacity on every exit
- **WHEN** the hold is cleared, the owner is dropped, the child changes, reveal
  completes, or a later hold supersedes the current one
- **THEN** the live child opacity or equivalent visibility state is restored
  exactly once
- **AND** the cover texture is hidden or cleared so stale pixels cannot remain
  visible

#### Scenario: Cover cannot steal input
- **WHEN** the cover is visible during a hold
- **THEN** pointer, keyboard, focus, and accessibility behavior continue to
  belong to the underlying live widget or surrounding app as before
- **AND** the cover is not targetable as an independent control

### Requirement: RenderHoldOverlay scheduling stays caller-owned
`RenderHoldOverlay` SHALL own capture, cover visibility, warm, reveal, and
cleanup state, but it MUST NOT own animation detection, readiness predicates,
settle timers, or application workflow state. Consumers SHALL decide when to
hold, extend, warm live content, reveal early, or clear after settle.

#### Scenario: Hold composes with external settle
- **WHEN** a consumer pairs `RenderHoldOverlay` with a settle burst or other
  app-owned quiet window
- **THEN** the consumer drives hold and reveal through documented methods
- **AND** `gtk-lush-widgets` does not depend on `gtk-lush-settle` at runtime

#### Scenario: Warm under cover is explicit
- **WHEN** a consumer needs the live child to repaint before the cover is
  removed
- **THEN** the overlay exposes an explicit warm or restore-under-cover step
- **AND** clearing the cover before warm remains a deliberate caller decision

#### Scenario: User interaction can reveal early
- **WHEN** a consumer detects direct user scroll or interaction while a hold is
  waiting to reveal
- **THEN** the consumer can reveal or clear the hold immediately
- **AND** cleanup still restores the live child and hides the cover

### Requirement: Public documentation and tests prove widget behavior
Every public item in `gtk-lush-widgets` SHALL be documented under the GTK Lush
engineering bar. Observable behavior MUST have runnable doctests, unit tests,
or headless widget tests as appropriate. The crate MUST keep
`#![forbid(unsafe_code)]` and `#![deny(missing_docs)]`.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public widget, property, method, state, or error type is added
  without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: ClipBin widget tests cover geometry
- **WHEN** `cargo test -p gtk-lush-widgets` or the family widget lane runs
- **THEN** tests cover empty, populated, constrained, child replacement, clip,
  and builder/property states without linking LushText

#### Scenario: RenderHoldOverlay widget tests cover lifecycle
- **WHEN** the render-hold tests run
- **THEN** they cover successful capture, failed capture, superseded hold,
  warm-under-cover, early reveal, drop cleanup, opacity restoration, and
  non-targetable cover behavior

#### Scenario: README teaches visual proof limits
- **WHEN** the README is rendered
- **THEN** it explains that render-hold preserves already-rendered native
  pixels temporarily
- **AND** it warns that screenshot/pixel proof is still required for
  toolkit-rendered effects

### Requirement: LushText shrinkable bin migrates to ClipBin
LushText SHALL replace `LushtextShrinkableBin` with `gtk-lush-widgets::ClipBin`
for the main content wrapper or reduce the app-local type to documented
compatibility glue with a removal task in the same change. The migration MUST
preserve short-window behavior, status bar visibility, template loading, type
registration, and warning-free allocation.

#### Scenario: Short window keeps persistent chrome
- **WHEN** LushText runs at the normal-mode minimum supported height after the
  migration
- **THEN** the status bar and fixed chrome remain visible
- **AND** optional editor/sidebar/search content yields or clips in the same
  visible regions as before

#### Scenario: Template uses the reusable widget
- **WHEN** generated resources and widget type registration are inspected after
  the migration
- **THEN** the window content wrapper uses `ClipBin` or a documented temporary
  alias to it
- **AND** duplicate app-local clipping logic is removed

#### Scenario: Constrained content has no new scrollbars or warnings
- **WHEN** the shell is tested with many tabs, open search, side surfaces,
  minimap, and constrained height
- **THEN** no unintended root-level scrollbars appear
- **AND** GTK, Libadwaita, GDK, renderer, and accessibility warning gates stay
  clean

### Requirement: LushText minimap reflow freeze migrates to RenderHoldOverlay
LushText SHALL replace the app-local native minimap reflow-freeze owner with
`gtk-lush-widgets::RenderHoldOverlay` or a documented compatibility adapter
whose only job is to connect LushText-specific timing to the reusable widget.
The migration MUST preserve native `GtkSourceMap` rendering, marker layering,
read-only behavior, focus behavior, navigation behavior, warm-under-cover,
early reveal, and final settled geometry.

#### Scenario: Native minimap appearance is preserved
- **WHEN** the minimap is visible before, during, and after a sidebar
  width-reflow burst
- **THEN** users see the same native `GtkSourceMap` viewport highlight and
  marker layering as before
- **AND** no app-owned replacement highlight, recolor, or restyled substitute
  is introduced

#### Scenario: Early reveal preserves user scroll
- **WHEN** the user scrolls or navigates the minimap while a render hold is
  waiting to reveal
- **THEN** the hold is revealed or cleared through the reusable overlay
- **AND** the live source map is visible and synchronized with the user action

#### Scenario: Minimap proof uses pixels
- **WHEN** the render-hold migration affects minimap rendering, source-map
  geometry, sidebar animation, or visual proof policy
- **THEN** visual-geometry evidence includes passing screenshot-derived
  pixel-anchor and animation-stream scenarios
- **AND** final-settle-only or app-geometry-only evidence is insufficient

### Requirement: Widget migration preserves proof gates
The widget extraction SHALL preserve LushText's adaptive geometry, minimap, and
warning-free contracts. The phase MUST pass family crate tests, widget tests,
visual-geometry proof, warning scans, and delegated GTK internals,
responsiveness, architecture, and comments reviews before archive.

#### Scenario: Delegated reviews cover widget risks
- **WHEN** the widget migration is implementation-complete
- **THEN** focused delegated reviews examine custom widget lifecycle,
  measurement/allocation/snapshot behavior, render-hold cleanup, visual proof,
  architecture boundaries, and comments
- **AND** actionable findings are fixed before the phase is marked complete

#### Scenario: Visual-sensitive changes have bounded evidence
- **WHEN** `ClipBin` or `RenderHoldOverlay` changes affect rendered geometry
  or minimap animation
- **THEN** the proof run preserves bounded screenshots, geometry summaries,
  crop artifacts, warning logs, and skip/failure reasons
- **AND** no artifact exposes user document text, note bodies, drafts,
  local-history contents, or private persistence identifiers
