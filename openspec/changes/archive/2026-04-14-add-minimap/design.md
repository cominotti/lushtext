## Context

LushText's editor page already owns the tab-local concerns that a minimap depends on: the `GtkSourceView` buffer, bookmark projection, in-tab search state, file-size policy, eviction state, and editor-local notifications. The current `editor-page.ui` template renders a single scrolled source view plus the bottom search revealer, so adding a minimap means extending the editor-page layout rather than inventing a new window-level shell.

The user brief in [docs/next/minimap.md](/var/home/danilo/Workspace/github/cominotti/lushtext/docs/next/minimap.md) asks for more than a decorative text overview. The minimap needs to become a navigation surface that can also surface bookmarks, search hits, modified-since-save regions, and long-line warnings while staying responsive on large files. That means the design has to balance three pressures: stay native to GtkSourceView, respect the `editor_page/` ownership boundaries, and reuse the repo's existing large-file guardrails instead of forcing a second heavy rendering path onto already expensive documents.

## Goals / Non-Goals

**Goals:**
- Add a tab-local minimap that can be toggled on and off without changing the outer workspace or properties split-view contracts.
- Reuse the active editor buffer and scroll state so the minimap always reflects the current document and viewport.
- Show semantic markers for editor-local signals that matter for navigation: bookmarks, active in-tab search matches, modified-since-save ranges, and long lines.
- Keep the feature bounded by existing responsiveness policy, especially for very large files.
- Keep the implementation modular so `imp.rs` does not become another mixed workflow file.

**Non-Goals:**
- A full custom text-rendering minimap in the first iteration.
- Workspace-wide search panel integration or any minimap behavior that depends on `ui/search_panel/`.
- A broad new preferences surface beyond the persisted minimap setting and width key needed by the feature.
- Pixel-perfect semantic syntax-region painting for every lexer token category in the first shipped version.

## Decisions

### 1. Keep minimap ownership inside `ui/editor_page/` and add a dedicated workflow module

The minimap is a tab-local concern, so the primary implementation should live in a new sibling workflow module such as `ui/editor_page/minimap.rs`, with only explicit state handles added to `imp.rs`. This follows the local `AGENTS.md` rule for new tab-local workflows and keeps buffer projection, save/load resets, and search integration close to the existing editor-page contracts.

Alternatives considered:
- Implement the feature from `ui/window/`: rejected because the minimap depends on editor-page-local state and would blur window/editor ownership.
- Grow `imp.rs` directly: rejected because it would mix layout, marker projection, and interaction logic into an already busy state file.

### 2. Place the minimap in a dedicated right-side editor child, not as a floating overlay over text

The editor template should evolve from a single scrolled editor child into a horizontal editor body that contains the main `GtkScrolledWindow` plus a minimap container on the right. The bottom search revealer remains where it is today. This makes the minimap's width explicit, keeps pointer input and hit testing simple, and avoids obscuring document text underneath an overlayed widget.

Alternatives considered:
- Use the existing `GtkOverlay` as a floating minimap layer: rejected because it would either cover text or require awkward margin choreography on the main source view.
- Add a new outer paned widget: rejected because minimap width is not a user-resizable shell pane and should not complicate the window-level split-view math.

### 3. Use `GtkSourceMap` for the text overview and a companion semantic-marker strip for richer signals

The first implementation should use `sourceview5::Map` bound to the existing `source_view()` as the base minimap. Beside or atop that map, LushText should render a thin semantic-marker strip that paints normalized line ranges for bookmarks, active in-tab search matches, modified-since-save regions, and long-line warnings. This preserves GtkSourceView's built-in overview and scroll coupling while giving LushText a controlled surface for markers that may be too subtle or too limited if expressed only through generic source marks.

Alternatives considered:
- Rely exclusively on `GtkSourceMark` visibility inside `GtkSourceMap`: rejected as the only mechanism because it gives too little control over marker density, color grouping, and non-mark-based signals like long-line warnings.
- Build a full custom minimap renderer immediately: rejected because it duplicates text rendering, increases performance risk, and is unnecessary while `GtkSourceMap` already handles the hard parts of buffer overview and scroll sync.

### 4. Model minimap inputs as explicit editor-local projections

The minimap should consume explicit editor-local projections rather than reaching into unrelated modules at paint time:
- Bookmarks come from the existing bookmark projection state already owned by `editor_page`.
- Search markers come only from the in-tab `LushtextSearchBar` / `GtkSourceSearchContext`, not from the workspace-wide search panel.
- Modified-since-save regions come from a new line-range tracker owned by the editor page and cleared on successful load/save/discard.
- Long-line warnings come from a debounced scan of the active buffer against the feature's line-length threshold.

Each projection should normalize into line-oriented ranges so the semantic-marker strip can paint from one merged model instead of learning about bookmarks, search, and save semantics independently.

Alternatives considered:
- Query live GTK widgets ad hoc during every paint: rejected because it couples rendering to many unrelated objects and makes testing difficult.
- Treat workspace search as another marker source: rejected because the `editor_page/` contract explicitly excludes workspace-wide search workflows.

### 5. Gate minimap availability with existing file-size policy, not "low value" viewport heuristics

The minimap should only materialize when all of the following are true:
- The global `show-minimap` preference is enabled.
- The active document is in a file-size tier where minimap cost is acceptable.
- The tab is not currently evicted.

The conservative default is to disable the minimap once `FileSizeCheck::syntax_enabled()` is already false, which aligns the feature with LushText's existing "disable expensive secondary presentation work above 10MB" policy. When the user preference stays enabled, supported tabs should keep the minimap visible even if the whole document already fits in the viewport. This matches the requested product contract more closely than trying to hide the minimap when the app decides it is "low value".

Alternatives considered:
- Hide the minimap when the full document fits: rejected because it makes the toggle feel inconsistent and removes the spatial overview precisely when the user explicitly asked to keep it on.
- Permanently turning the preference off when a document is unsupported: rejected because availability is document-specific, not a global preference change.

### 6. Use GtkSourceMap's native viewport indicator and style it explicitly

The minimap should use `GtkSourceMap`'s own viewport slider or overlay as the source of truth for the visible-region rectangle. The earlier custom-drawn rectangle approach looked mathematically plausible but drifted from the map's real rendered geometry because the source map has its own margins, scaling, and internal layout behavior. Styling the native viewport indicator more aggressively is the most reliable way to match GNOME Text Editor's visual behavior while keeping the visible-region math owned by the toolkit that draws the map.

Alternatives considered:
- Draw our own viewport rectangle from editor scroll fractions: rejected because it can easily become visually wrong even when the underlying fractions are "correct" for the editor scroller.
- Replace the map entirely with a custom-drawn minimap: rejected because the rest of the overview behavior already works well with `GtkSourceMap`.

### 7. Bundle known-good default source schemes for map-overlay visibility

The native viewport indicator is only as visible as the active GtkSourceView style scheme's `map-overlay` definition. Because LushText currently depends on whatever platform scheme files happen to be installed, the same minimap can look correct on one machine and disappear on another. To make the shipped default experience deterministic, LushText should bundle the official GtkSourceView `Adwaita` and `Adwaita-dark` scheme files in its own resources and prepend that resource path to `GtkSourceView::StyleSchemeManager` during startup. That keeps the default `style-scheme = "Adwaita"` choice predictable without trying to override arbitrary third-party schemes.

Alternatives considered:
- Depend entirely on platform-provided schemes: rejected because it already produced an invisible viewport indicator on this machine.
- Bundle every upstream scheme immediately: unnecessary for this fix because the visibility contract only needs the shipped defaults to be reliable.

### 8. Update marker projections with debounced editor-page signals and explicit lifecycle resets

Minimap state should reset on file load, save, discard, reload, and tab disposal the same way other editor-page projections do. Search markers update when the in-tab search session attaches, detaches, or changes query. Modified ranges update from user-edit signals and clear on successful save. Long-line markers update from debounced scans so typing and bulk loads do not repaint the marker strip for every individual buffer mutation. This mirrors the generation-counter patterns already used elsewhere in the codebase and keeps the minimap's live state understandable.

Alternatives considered:
- Immediate full recompute on every buffer change: rejected because it risks visible typing jank.
- Lazy updates only when the minimap is clicked: rejected because the feature would stop acting like a live navigation overview.

## Risks / Trade-offs

- [A second source-view-based overview can add measurable cost on large files] -> Disable the minimap once the active file crosses the same tier that already disables syntax highlighting, and keep the preference global rather than forcing the map into unsupported documents.
- [Semantic markers could sprawl across unrelated modules] -> Normalize every signal into editor-local line ranges and keep the marker strip as the only renderer that understands combined minimap markers.
- [Modified-since-save tracking can drift if resets are incomplete] -> Hook resets into the existing load/save/discard success paths instead of treating change tracking as a purely visual concern.
- [Wrapped lines and viewport math can make click-to-line mapping or overlay placement feel imprecise] -> Base both navigation and the overlay rectangle on adjustment fractions and `GtkSourceMap` overview scrolling rather than inventing a second text layout model.

## Migration Plan

Add the new GSettings keys (`show-minimap` and `minimap-width`) with safe defaults, extend `editor-page.ui` and `editor_page` state to host the minimap container, and wire the action/shortcut during the same change so the feature is complete when shipped. No user data migration is required. Rollback is straightforward: the app can ignore the new keys and remove the editor-page minimap container without touching persisted documents, drafts, or sidecar data.

## Open Questions

None blocking. A later follow-up can decide whether `minimap-width` becomes a user-facing preference row or remains an internal persisted tuning knob.
