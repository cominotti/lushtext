# Baseline and behavior inventories

## Baseline revision and portfolio order

- Revision: `a5db36f201d896e690f9b4ef37982ce2e8b0d3d2` on clean `main`.
- The seven prerequisite portfolio changes are present in history before this
  change. The latest prerequisite commits are `367eb00` (shared fuzzy ranking)
  and `3e7a062` (responsive workspace watcher restarts); earlier history contains
  the draft pipeline/cleanup, live editor memory, replace-preview, and editor
  main-thread work.
- `make pre-commit`: passed.
- `make test-widget-headless`: passed 913 tests with no `FLAKY:` recovery.
- `make check-automation-docs`: passed.
- `make automation-client-self-test`: passed.
- `make accessibility-smoke`: passed with AT-SPI anchors and focus artifacts.
- `make visual-smoke`: passed the full representative matrix, including wide,
  compact, constrained-height, workspace, notes, bookmarks, palette, theme, and
  recovery states.
- `make visual-geometry-smoke`: passed the same-session geometry and pixel proof
  matrix.

Baseline automation-contract checksums:

| Surface | SHA-256 |
| --- | --- |
| `services/action_catalog/mod.rs` | `fc726a032cf7885ca96578613d631149677cd2b0e510fbd1c98a12ec83e86a3d` |
| `ui/automation.rs` | `7effeec85b92c6e0a9b836d139f1c1ab1dcf55f2080c12535c9acb3f3668d7e8` |
| `scripts/lushtext-automation.py` | `3e3b90e18d8fe33e30ae4aa0feaf8758a5e843d3323413b69d6270ac9f3444f2` |
| `docs/automation.md` | `3c571c34ede4cfe5dca429dda30598893116ac2222876fbb56275708d18a8209` |
| `docs/automation-reference.md` | `c36fe2cfb5c79a383b8d06520ceab05960c68c1d2da75a91d1ae2d885cffada3` |

## Window notes inventory

### Window actions and public routes

The action catalog and `window/actions.rs` expose these notes routes, all owned
by `window/notes`: `toggle-bookmark`, `notes-toggle-bookmark`,
`edit-bookmark-label`, `next-bookmark`, `prev-bookmark`, `show-bookmarks`,
`open-document-note`, `notes-open-document-note`, `open-folder-note`,
`notes-open-folder-note`, `show-notes`, `notes-show-notes`,
`set-notes-browser-query`, `select-notes-browser-row`, and
`open-notes-browser-selection`. Keyboard routes include Ctrl+F2,
Ctrl+Shift+F2, F2, Shift+F2, Ctrl+Alt+B, and Ctrl+Alt+A. Palette activation
routes through `activate_palette_note_target` and browser automation routes
through the active browser state.

### Shared facade and callbacks

- `wire_note_callbacks` connects editor bookmark/note changes to menu state,
  debounced persistence, and palette-source refresh.
- `resolve_notes_for_editor`, `reset_notes_after_save_as`,
  `open_editor_note_snapshots`, `open_editor_for_path`,
  `require_saved_editor`, workspace-folder target helpers, and notes-menu
  availability coordinate workflows shared by bookmarks, editors, and browser.
- Menu availability is rebuilt from active-editor bookmark state plus current
  workspace scope; browser actions are enabled only while an active browser
  target is alive.

### Bookmark workflow

- Toggle, label edit, validation/error display, next/previous navigation,
  bookmark-only browser, excerpts, raw preview target tagging, and activation
  return through the existing editor/window paths.
- Debounced bookmark persistence uses the editor-owned save generation. The
  completion accepts only the matching generation and leaves newer dirty state
  intact; persistence errors remain visible and retryable.
- Bookmark preview uses a separate browser preview generation. Live open-editor
  excerpts take precedence; background excerpt completions are rejected when
  the browser, selection, path, line, or generation changed.

### Document and folder note editors

- Document targets require a saved editor or explicit saved path. Folder targets
  preserve workspace identity and the zero/one/many-folder chooser behavior.
- Editor dialogs retain Edit/Render geometry, pre-rendering, save/clear/cancel
  responses, dirty confirmation, text-surface padding, first-focus behavior,
  and discard semantics.
- Saves keep the existing `spawn_blocking_then` service boundary and durable
  sidecar identity. Response enablement is refreshed by a superseding timer;
  stale callbacks cannot clear newer text or close a superseded dialog.

### Notes browser and command palette

- Browser state owns all entries, filtered indices, active selection, preview
  widgets, preview generation, search debounce, and dialog weak identity.
- States covered by the baseline are empty, few, dense, awkward path,
  constrained, no active editor, bookmarks-only, notes-only, and mixed
  categories. Search rebuilds sectioned sidebar rows without fake entries.
- Command-palette note refresh uses its own debounce and monotonically advancing
  generation; the latest accepted source replaces the palette model, and stale
  service completions are ignored.

### Migration and automation anchors

- `migrate_note_sidecars_after_rename` coordinates bookmark, document-note,
  folder-note, and local-history sidecars with the migration ledger. Failed or
  partial moves remain retryable.
- `reconcile_pending_migrations_on_startup` preserves the ledger generation and
  refreshes editor/menu/palette projections only after accepted reconciliation.
- Public automation remains anchored by the action rows above, the Notes browser
  snapshot/readiness fields, and AT-SPI anchors for dialog, search, result list,
  preview, close/open controls, bookmark rows, and empty/search-empty states.

### Final workflow split

- `notes/mod.rs` is the private facade for shared types, callback wiring,
  migration coordination, editor lookup, workspace targeting, menu state, and
  common dialog helpers.
- `notes/bookmarks.rs` owns bookmark actions, debounced persistence, standalone
  browsing, edit validation, excerpts, and bookmark preview generation.
- `notes/editors.rs` owns document/folder targeting, editor dialogs,
  Edit/Render state, save response refresh, and sidecar service calls.
- `notes/browser.rs` owns unified browsing, search/category projection,
  command-palette note refresh, preview selection, and target activation.
- Strict all-feature workspace Clippy passed. The focused headless `window::`
  widget slice passed all 282 tests with no flaky recovery or warning output.

## Adaptive-shell inventory

### Plain inputs and outputs

- Inputs: current allocated/restored width, `WorkspaceSidebarWidthPreset`,
  requested workspace visibility, requested properties visibility, explicit
  compact-surface owner, and Focus Mode state.
- Outputs: properties breakpoint maximum width, whether workspace width is
  consumed, pane/sheet presentation, compact-surface owner, and rendered
  workspace/properties visibility.
- Width policy constants cover the workspace and properties minimums, fixed
  right-pane fraction, protected center width, dual-pane overhead, normal-mode
  height, workspace breakpoint, and Open-button breakpoint.

### Breakpoints, settings, allocation, and focus

- `imp.rs` reads GSettings for requested visibility and the workspace preset,
  migrates legacy keys once, restores requested intent, and persists only
  explicit/settled user choices.
- `install_split_view_breakpoints` parses and installs properties-layout,
  workspace-collapse, and Open-button conditions once. Runtime properties
  threshold changes update only when the cached integer threshold changes.
- `size_allocate` compares the allocated width with
  `split_width_synced_for_width`; it clamps runtime fractions, preview width,
  and the cached properties breakpoint without writing GSettings.
- Applying a decision mutates `MultiLayoutView`, split-view visibility, and the
  bottom sheet in `imp.rs`, then restores editor focus only when a previously
  rendered focused surface closes.
- Sidebar transition settle is armed before the Adwaita visibility mutation;
  final reconciliation runs after the transition budget.

### Existing pure coverage

Pure tests cover every preset's breakpoint, requested-workspace budgeting while
compact arbitration suppresses it, passive compact shrink, explicit compact
workspace ownership, dual-pane center-width preservation, and representative
workspace width clamping. Widget and visual baselines additionally cover wide,
compact, constrained-height, both-requested, animation, and restored-intent
states.

## Workspace-section wiring inventory

### Factory lifecycle and row-owned data

- `setup` creates one reusable overlay, expander/content hierarchy, drag handle,
  open indicator, icon, label, focus button, insertion line, and inert DnD
  shield. Permanent focus-button routing resolves the live section from the
  recycled widget tree.
- `bind` clears prior binding/signal/registration state first, attaches the live
  `TreeListRow`/`FileTreeItem`, projects icon/label/open state, restores
  selection/focus controls, installs DnD row handoff, toggles the expander's
  internal gesture for file-versus-directory rows, applies accessibility, and
  triggers pending inline rename only for the current item.
- `unbind` clears the expander row, binding bag, signal bag, DnD registration,
  expanded accessibility hook, rename entry/label state, overlay/drop state,
  tooltips, accessible row metadata, and any matching section context target.
- Private object-data keys own only row-local signal/binding/registration bags;
  every `set_data` has bind/unbind clearing symmetry via `steal_data`.

### DnD, rename, context menus, and keyboard routes

- Workspace-folder reorder remains on the transparent row shield; TreeExpander
  owns disclosure. Bind/unbind delegates registration and cleanup to `dnd`.
- Inline rename keeps the current target in `context_target`, removes recycled
  entries, restores label visibility, and consumes `pending_rename` once.
- One section-owned file popover and one header popover are created during
  construction and unparented during `dispose`. Rebuild/refresh/dispose pop
  stale surfaces down through the shared lifecycle path.
- Pointer targeting uses `pick` plus ancestor `TreeExpander` resolution.
  Keyboard parity uses Menu and Shift+F10, current selection, realized row
  bounds, and scroll-to-focus fallback. Public automation entry points reuse
  those exact selection/header paths.
- File menu groups preserve Focus Folder, Local History, document/folder notes,
  create, rename/delete, folder reorder, and remove-from-workspace actions.
  Header actions preserve Add Folder, Open Folder Note, Rename Workspace, and
  Remove Workspace.

### Accessibility and disposal

- Bind projects role, bounded name/description, selected state, set position,
  expanded state, placeholder disabled state, reorder-handle metadata, and
  focus-button metadata.
- Directory rows install one expanded-state `SignalBag`; bind and unbind both
  clear it before reuse. Expanded notifications update only the current overlay
  and section through weak references.
- `dispose` invalidates async lifetime generations, stops watchers/timers,
  clears factory/model state, pops/unparents context and peek surfaces, and
  releases template-owned resources without retaining the section.

## Final validation and review

- `make test` passed 1,141 non-GUI tests and all 913 headless widget tests with
  no flaky recovery. The focused `window::` slice independently passed 282
  tests, and `make test-prop` passed all 29 bounded property tests.
- Strict all-feature workspace Clippy, `make check`, `make lint-advisory`, and
  workspace rustdoc with warnings denied passed. The advisory lane reported
  only its accepted baseline classifications.
- `make check-agent-docs`, `make check-automation-docs`, and
  `make automation-client-self-test` passed. Strict validation passed for this
  change and for all 104 OpenSpec changes/specifications.
- Accessibility smoke, the full representative visual-smoke matrix, and the
  same-session visual-geometry proof passed after refreshing their
  source-sensitive fingerprints.
- An isolated headless-Mutter runtime launch opened a saved Markdown file and
  the empty Notes dialog. Automation waits and the `No notes yet` AT-SPI anchor
  passed; LushText emitted no stdout or stderr warnings. Only known portal and
  PipeWire teardown noise occurred outside the app process.
- Responsiveness, scale/memory, and Rust hot-path reviewers found no actionable
  performance regressions. The split preserves bounded row materialization,
  recycled-row cleanup, off-thread I/O, and generation-checked completions.
- The architecture review confirms every changed Rust file remains a driving
  adapter except `adaptive_shell.rs`, which is plain GTK-free UI policy. No GTK
  types moved into services or models, and no manager, trait, widget, or new
  crate-wide API was introduced. The comment review found the surviving module,
  safety, generation, persistence, and geometry rationale sufficient.
- The learning review removed the stale `notes.rs` ownership rule and recorded
  the durable workflow boundaries in the nearest window/sidebar/crate guidance.
  No hook, rule, skill, or memory update is warranted.

Final automation-contract checksums match the baseline exactly for the action
catalog, D-Bus adapter, automation client, and both automation documents.
