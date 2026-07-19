## Why

The completed quality and boundedness portfolio materially improved LushText, but the final live-code review found a small set of release-only accounting, GTK ownership, and in-operation byte-bound gaps that can still cause large main-loop turns, false Replace All rejection, or avoidable memory spikes under supported extreme inputs. Closing every confirmed finding and fold-in recommendation in one final change preserves the architecture that already landed while making the current readiness claim reliable in release builds and failure paths.

## What Changes

- Make workspace-search retirement charge every destructive step in debug and release builds, verify real container deltas rather than self-reported counters, and promote mutating debug assertions into blocking lint policy.
- Replace the growing GTK-owned whole-buffer snapshot accumulator with bounded chunks whose final coalescing and rejected-payload destruction occur off GTK under existing admission; keep Local History Undo Restore text guarded through storage, replacement, cancellation, and final disposal.
- Make Replace All reclaim undo payload accounting after pre-rename failures, enforce undo-read limits inside ingestion, and return only bounded diagnostic samples plus the compact path evidence the UI actually consumes.
- Enforce separate palette file-index build and installed-result byte ceilings during traversal before retaining raw paths, canonical identities, directory state, scan batches, or pending work that would exceed them.
- Route the bookmark-only browser through the existing bounded Browse Notes source, query, projection, and disposal path while preserving bookmark-only scope, activation, empty, truncation, and accessibility behavior.
- Enforce the recent-document metadata cap inside the read operation and base minimap wrapped-layout admission on the conservative live-buffer byte estimate rather than Unicode scalar count.
- Share immutable workspace-folder and Markdown render-context snapshots across search requests, table cells, and image work, and resolve relative Markdown image candidates lazily only after image admission instead of allocating one full candidate vector per embed.
- Add direct unit, integration, property, release-semantic, headless widget, and performance-smoke evidence for every corrected boundary; leave `make check`, `make lint-advisory`, strict OpenSpec validation, and the full relevant test stack clean.
- Keep the work inside existing model policy, service, UI workflow, filesystem, disposal, and test boundaries; do not add a crate, dependency, generic manager, global scheduler, persisted-format migration, GTK Lush public API, or new user-facing workflow.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: Make search retirement release-invariant and make whole-buffer snapshot accumulation, coalescing, and rejected ownership bounded per GTK turn.
- `local-history`: Keep restore-safety and Undo Restore document bodies guarded and worker-disposable without cloning or finally dropping large plain text on GTK.
- `search-replace-safety`: Make undo-cap accounting exact across failures, enforce bounded undo ingestion, and bound apply/undo result metadata crossing back to GTK.
- `command-palette-source-groups`: Enforce the file-index working-set byte budget incrementally across output, canonical, directory, pending, and folder ownership.
- `workspace-notes`: Reuse the bounded unified Notes inventory and query path for the dedicated bookmark browser.
- `editor-minimap`: Apply wrapped-layout safety using a conservative current byte estimate for saved, modified, and untitled multibyte buffers.
- `recent-open-popover`: Enforce recent-document metadata limits inside the read and preserve recovery behavior when the file grows concurrently.
- `markdown-preview-local-images`: Share workspace scope ownership and perform relative image candidate expansion lazily after bounded image admission.
- `rust-linting-policy`: Block side effects inside debug-only assertions and fully classify or clean the current advisory-lint drift.
- `performance-regression-coverage`: Add release-semantic, failure-path, long-path, large-body, dense-browser, and bounded-metadata evidence for the final readiness closeout.

## Impact

Implementation will primarily affect search retirement, buffer snapshotting and save consumers, Local History restore ownership, Replace All/Undo services and window projection, palette indexing, bookmark browsing, recent-document loading, minimap availability, Markdown image resolution, lint policy, and their existing test/benchmark surfaces. User-visible behavior, file contents, undo/recovery semantics, persisted JSON formats, automation contracts, dependencies, and GTK Lush public APIs remain compatible; the intended impact is tighter release behavior, lower worst-case GTK work, and truthful in-operation bounds.
