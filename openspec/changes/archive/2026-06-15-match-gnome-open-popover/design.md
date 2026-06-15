## Context

LushText currently exposes a header `open_button` that directly activates `win.open-file` and opens the normal file chooser. GNOME Text Editor instead uses a flat header `GtkMenuButton` whose child is an `Open` label plus a down chevron in wide layouts and a folder icon in narrow layouts. That button owns a custom `GtkPopover` with a search entry, a compact file-chooser button, a separator, and a stack that switches between a recent-document list and a no-recents empty state.

The upstream GNOME Text Editor implementation studied for this change is commit `f00b4f5c2f5e03e4833cf14ad58cb04d31480f98`. Relevant upstream files are `src/editor-window.ui`, `src/editor-open-popover.ui`, `src/editor-open-popover.c`, `src/editor-sidebar-row.ui`, `src/editor-sidebar-model.c`, `src/editor-search-model.c`, and `src/style.css`. Upstream uses `max-content-height=600` for the recent-list scroller, but it does not cap the model to 10 rows. For LushText we will keep the full recent list searchable while making exactly 10 recent rows visible at default scale before the item region scrolls.

## Goals / Non-Goals

**Goals:**
- Match GNOME Text Editor's Open button visual structure and popover behavior closely enough that users recognize the same surface.
- Preserve the existing file chooser workflow through a fixed chooser action inside the popover and through the existing `Ctrl+O`/action path.
- Add a searchable, newest-first recent-document model that can contain more rows than are visible.
- Make 10 rows visible at default text scale in the popover list region, with overflow scrolling only inside that region.
- Prove the surface with broad model, widget, keyboard, accessibility, visual, and smoke coverage across empty, populated, dense, awkward, and 720p-constrained states.

**Non-Goals:**
- Do not replace the command palette's workspace-file or open-tab search.
- Do not change tab duplicate detection semantics beyond routing recent-row activation through the existing `open_document()` workflow.
- Do not expose document contents, draft bodies, notes, or local-history data in recent-list persistence, automation snapshots, or visual artifacts.
- Do not depend on the desktop-wide recent-file database as the only source of truth; LushText keeps app-owned persistence so Flatpak/sandbox behavior remains predictable.

## Decisions

### Use a Custom Popover Widget

Implement a dedicated `LushtextOpenPopover` GTK widget rather than a `GtkPopoverMenu`. The GNOME surface is not a menu model: it contains a live search entry, a fixed chooser button, a list view, custom empty state, row-level remove controls, and keyboard behavior that moves focus between the entry and rows. A custom widget also gives tests direct access to state and geometry helpers.

Alternative considered: build a `PopoverMenu` from `gio::Menu`. That would be simpler for static actions but cannot naturally host the searchable list and row-specific behavior.

### Keep Persistence in an App-Owned Recent-Documents Service

Add a small recent-document model and service, likely `model/recent_document.rs` plus `services/recent_documents.rs`, persisted through `json_store` and the filesystem boundary. Store only local paths plus last-opened metadata needed for ordering and display. Load on window startup with `spawn_blocking_then`, update after successful file-backed opens, and save through durable low-stakes JSON persistence.

Alternative considered: use GTK/GIO's global recent manager or GNOME Text Editor's private XBEL shape. App-owned JSON fits the existing LushText metadata stack, keeps recovery behavior consistent with search history/session files, and avoids surprising privacy or sandbox differences.

### Present Open Documents Separately From Recents

The popover list should exclude already-open file-backed documents from the recent rows, mirroring GNOME Text Editor's intent to avoid duplicate clutter. If a stale row is activated for a path that is already open, the existing `open_document()` duplicate detection still wins and focuses the existing tab.

Alternative considered: show open tabs in the recent list. LushText already has a tab bar and command palette open-tab group, so duplicating them in the Open popover would make the GNOME-matching surface noisier.

### Derive the 10-Row Viewport From Stable Row Geometry

Rows should use a stable two-line GNOME-style layout: title, subtitle/path context, optional age text, and a flat circular remove button. The list scroller should be bounded to the height of 10 default-scale rows, including row margins. Overflow rows remain in the model and scroll inside the item region. The search/header area, separator, popover chrome, and chooser action stay fixed.

At 720p default-scale geometry, the total popover height should fit below the app header bar. The design should verify this with widget allocation assertions and a live visual scenario rather than relying only on arithmetic.

Alternative considered: copy upstream's `max-content-height=600` exactly. That preserves GNOME's source constant but makes the visible row count font/theme-dependent and can exceed the intended 10-row contract once header/search chrome is included.

### Match GNOME Keyboard Behavior

Opening the popover clears prior search text, resets list scroll to the top, and focuses the search entry. Typing filters title/subtitle/path with case-insensitive prefix, substring, and fuzzy matching. `Enter` opens the first visible match. `Down` from the search entry moves focus to the first row when any result exists. `Up` from the first row returns focus to the search entry. `Escape` closes the popover through GTK popover semantics and restores focus to the previously active editor when possible.

Alternative considered: treat the popover like a passive menu and rely on default list keynav only. That would miss the behavior GNOME explicitly designed for Ctrl+K-style recent search.

### Preserve Existing Open-File Action Semantics

Keep `win.open-file` as the file chooser action for `Ctrl+O`, automation, command palette, and the chooser button inside the popover. Add a separate action or widget method for opening/focusing the recent Open popover, likely used by the header menu button and `Ctrl+K`. The file chooser path must pop down the open popover before presenting the dialog.

Alternative considered: retarget `win.open-file` to open the popover. That would break automation and the expected `Ctrl+O` behavior where users want the file chooser directly.

### Treat Tests as Part of the Feature

This change needs a state-matrix test plan, not only a happy-path widget test. Coverage should include:
- Pure model/service tests for ordering, deduplication, missing-file pruning, search scoring, privacy-safe persistence, and open-tab exclusion.
- Widget tests for empty, one-row, representative, 10-row, 11-plus-row, awkward-label, stale-row, and constrained-width/height states.
- Keyboard tests for focus, Enter, Up/Down, Escape, search clearing, and file chooser button routing.
- Accessibility checks for stable names/roles of the Open button, search entry, chooser action, list rows, remove controls, and empty state.
- Visual geometry proof for GNOME-like styling, 10 visible rows, item-region-only scrolling, and 720p fit with the LushText header/tab/status chrome present.

### Policy Resolutions

Retain the row-level remove control in the first implementation. This matches GNOME Text Editor's recent-list affordance and gives users a direct privacy escape hatch without adding a separate preferences workflow.

Record successful explicit local file-backed opens from the file chooser, recent rows, sidebar/workspace activation, command palette file activation, desktop activation, and CLI activation. Do not reorder recent history during session restore, and never record failed loads or unsupported non-local URIs.

Use GNOME Text Editor's `400sp` compact breakpoint for the header Open button. LushText keeps the same wide `Open` plus chevron presentation above that threshold and the same folder-symbolic presentation below it.

## Risks / Trade-offs

- Row height drift across themes or text scaling -> Use stable row layout constraints and test default-scale 10-row fit separately from accessibility text-scaling behavior.
- Recent-file persistence can leak path information -> Store only local paths already selected by the user, respect future privacy controls, avoid document contents, and keep artifacts bounded.
- File existence checks can block UI -> Load/prune recent entries on a background thread and reconcile UI models on the main thread.
- Popover focus can fight file chooser or dialog focus -> Pop down the Open popover before opening the chooser and route recent-row activation through existing window workflows.
- Visual tests may be flaky in live sessions -> Keep widget allocation tests as the deterministic base and use visual smoke for rendered style/geometry evidence.

## Migration Plan

1. Add the model/service and read missing recent-document storage as an empty list.
2. Introduce the Open popover widget behind the existing header location while preserving `win.open-file`.
3. Wire successful file-backed opens to update recent persistence.
4. Add tests and documentation updates alongside action/catalog/automation anchor changes.
5. If rollback is needed, keep the recent-document JSON as low-stakes unused app data and restore the direct header button while preserving `win.open-file`.

## Open Questions

None. The initial policy questions are resolved above.
