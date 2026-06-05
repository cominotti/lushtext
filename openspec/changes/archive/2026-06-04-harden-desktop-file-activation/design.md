## Context

LushText receives desktop and CLI document launches through `ApplicationImpl::open`, which currently converts each `gio::File` to `file.path()` and passes path-backed inputs to `LushtextWindow::open_document`. `open_document` is intentionally the single document-opening authority, so it also handles sidebar activation, command-palette activation, file chooser opens, duplicate-tab focus, and startup session restore.

That single path works for ordinary documents, but it does not currently distinguish why an open request arrived or whether an existing matching tab is a successfully opened document, a pending load, or a visible failed-load placeholder. A restored missing-file tab can briefly or permanently reserve the same path in duplicate bookkeeping. If a desktop activation arrives for that path, the duplicate guard can focus the stale failed tab instead of opening the explicit file the user selected.

The adjacent robustness gap is at the same boundary: desktop metadata uses `%U`, so GLib may deliver URI-shaped `gio::File` values. When `file.path()` is `None`, the current code silently skips that input instead of reporting that the activation could not be opened.

## Goals / Non-Goals

**Goals:**

- Make desktop/CLI/file-manager activation always open and focus each explicitly requested path unless a successfully loaded or still-pending real tab already owns that document.
- Preserve failed tabs and their inline error messages while preventing those tabs from blocking later explicit opens.
- Preserve normal duplicate-tab behavior for successful documents, canonical duplicates, in-app sidebar opens, command-palette opens, and multi-file activation.
- Surface non-path `gio::File` activation failures through visible status or inline feedback instead of silently ignoring them.
- Add regression tests that exercise the real `ApplicationImpl::open` path, session restore ordering, stale failed-tab placeholders, URI inputs, multi-file activation, and reused-window behavior.

**Non-Goals:**

- Do not add network document loading or remote URI editing.
- Do not change the curated MIME type list or user default-application registration.
- Do not delete failed tabs automatically when they still provide useful user-visible diagnostics.
- Do not weaken canonical duplicate reconciliation for symlinked or equivalent local files.

## Decisions

### Track Editor Load State Explicitly

Add an editor-scoped load state that can represent at least `Untitled`, `Loading`, `Loaded`, and `Failed`. `load_file_async` sets `Loading`, successful application sets `Loaded`, and load failure sets `Failed` before callbacks publish window-level feedback.

Rationale: duplicate detection needs a stable application state, not inference from `file_path`, `file_size`, title text, buffer modified state, or inline-alert labels. The existing failure cleanup clears `file_path` for many failed loads, but preserved modified failed tabs and startup races can still leave the window needing to know whether a path claim is reliable.

Alternative considered: infer failed state from the info bar title `Could Not Open File`. That would couple duplicate semantics to localized/user-facing text and would fail if another warning is active.

### Split Open Intent From Open Mechanics

Keep `open_document` as the default in-app open behavior, but introduce an explicit activation-aware entry point or option used by `ApplicationImpl::open`. This activation intent keeps normal duplicate behavior for loaded/pending documents while allowing a matching failed-load placeholder to be bypassed so a new tab is created and focused.

Rationale: sidebar clicks, command-palette opens, file chooser opens, and session restore can continue using the conservative duplicate path. Desktop/CLI activation is a stronger user intent because the user just selected a specific file outside the current window context.

Alternative considered: make all open requests bypass failed tabs. That is simpler, but it may surprise in-app workflows by piling up multiple visible failure placeholders for repeated sidebar or palette actions.

### Do Not Let Failed Placeholders Own Duplicate Bookkeeping

When a load fails, remove provisional path keys from `open_paths` before preserving or clearing editor identity. If a modified failed tab must keep its buffer and visible error, it should not continue to be treated as the successfully open owner of that path. Session collection should avoid persisting a failed placeholder as a clean file-backed tab unless it represents intentional unsaved content that still has draft/session safety.

Rationale: `open_paths` is a duplicate guard for real open documents. Failed placeholders are diagnostic surfaces, not durable file identities.

Alternative considered: keep failed tabs in `open_paths` and teach activation to search around them. That leaves stale state in a shared guard and increases the chance of future regressions in sidebar, command palette, save-as, and close paths.

### Report Non-Path Activation Inputs

For each `gio::File` passed to `ApplicationImpl::open` where `path()` is `None`, publish a visible activation error that includes the URI when available, and continue processing any remaining files. Do not create a fake editor tab for unsupported URI inputs unless the implementation can safely route them through the normal local-file load pipeline.

Rationale: silent no-op makes desktop/portal failures look like app launch failures. A status/inline error gives the user and tests a concrete observable outcome while avoiding unsupported remote editing semantics.

Alternative considered: download or stream URI contents into an untitled buffer. That would introduce remote I/O, save-back ambiguity, and security expectations outside this change.

### Regression Coverage Is Part Of The Design

Add widget tests close to the existing `crates/lushtext/tests/widget/app.rs` activation tests because those already exercise `ApplicationImpl::open`, desktop metadata forwarding, existing-window reuse, duplicate canonical paths, invalid paths, and explicit selection after restore. Add test seams only where needed to deterministically create pending/failed load states; prefer direct state construction over sleeps where possible.

The minimum regression matrix should cover:

- explicit activation opens and focuses a file when a restored failed placeholder for the same path exists;
- explicit activation remains selected after session restore when the restored tab later fails;
- repeated activation for a successfully loaded same file focuses the existing tab rather than creating duplicates;
- multi-file activation continues processing valid files after an unsupported URI input;
- unsupported non-path `gio::File` inputs show user-visible feedback and do not create bogus tabs;
- failed activation does not poison `open_paths`, even if the failed tab preserves modified buffer content;
- canonical/symlink duplicate behavior still deduplicates successful documents.

## Risks / Trade-offs

- Load state can drift from actual editor identity if not updated on every load, save-as, evict, close, and retry path. Mitigation: keep the state changes inside `load_file_async` / load result application and add tests for retry and failed-then-successful reopen.
- Activation-aware duplicate behavior could accidentally create duplicate tabs for a file that is merely slow to load. Mitigation: only bypass tabs whose load state is `Failed`; keep `Loading` as a valid duplicate owner.
- Non-path URI feedback could be noisy during multi-file activation. Mitigation: report each unsupported URI clearly but continue opening valid local files.
- Session persistence changes can affect draft recovery. Mitigation: test failed placeholders separately from modified draft-restored editors and ensure draft/session recovery specs remain intact.

## Migration Plan

No data migration is required. Existing session files may contain paths that fail on next startup; after this change, those restored failed placeholders remain visible but no longer block a later explicit activation of the same path.

Rollback is limited to reverting the activation entry point and editor load-state changes. Since no persistent schema changes are required, rollback should not require user data conversion.

## Open Questions

- Should file chooser opens use the activation-aware bypass or the conservative in-app duplicate path? The default design keeps file chooser opens conservative unless implementation reveals that portal chooser selection has the same stale-placeholder behavior as desktop activation.
- Should unsupported URI feedback be status-bar-only or create an untitled error tab? Status-bar feedback is the default design because it avoids fake document state, but implementation may choose an inline diagnostic if it can do so without implying the URI is editable.
