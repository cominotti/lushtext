# Session Restore Wiring

## Status: Implemented

Session restore is wired into the GTK application lifecycle. The remaining work
for this area is reliability hardening, smoke coverage, and future UX polish,
not initial lifecycle integration.

## Current Wiring

Startup restore runs from the window after construction. `draft_service` loads a
single `RestoreState` containing:

- `session.json` tab state
- `drafts/manifest.json`
- preloaded draft bodies or skip markers
- grouped recovery diagnostics
- whether orphan cleanup is safe for this startup

The window stores the manifest and preloaded draft map, applies grouped startup
diagnostics, restores file-backed tabs through `open_document()`, restores
untitled tabs through `new_tab()`, reapplies pinned state, and selects the
saved active tab when no CLI file argument takes priority.

File-backed cursor and scroll restoration is deferred until async file loading
has completed. Untitled draft content is applied after the tab is created.
Stale file-backed drafts remain document-scoped warnings so the grouped startup
message does not hide the safer per-document decision.

## Persistence

Session saves are debounced for ordinary tab open, close, switch, detach, and
pin changes. Close handling now uses an ordered async session-save path after
document and draft safety work has completed, so a slow filesystem does not
freeze one GTK main-loop turn. Failed debounced or close-time session saves stay
dirty and retryable against the newest generation.

Draft recovery is handled separately from session layout. The periodic autosave
timer remains the backstop, and a first-dirty debounce writes early recovery
data after the first edit in an editing cycle while reusing the same chunked
snapshot and background-write machinery as the normal autosave path.

## Reliability Evidence

The cheap validation tier lives in service, integration, property, and widget
tests:

- malformed session and draft metadata produce diagnostics without deleting
  unrelated recovery state
- partial draft-manifest repair restores only entries whose identity can be
  proven
- first-dirty autosave covers small buffers, large chunked buffers, in-flight
  coalescing, and failed-write retry
- close-time session-save failure produces visible feedback

The host-sensitive tier lives in `make crash-recovery-smoke`. That lane launches
the real app with isolated XDG and `LUSHTEXT_DATA_DIR` state, creates
file-backed and untitled draft/session recovery data through GTK, terminates the
process with `SIGKILL`, relaunches with the same data directory, and preserves
before/after metadata summaries, runtime logs, assertions, and screenshots.

## Future Follow-Ups

- Promote crash recovery smoke from scheduled/manual to PR gating only after it
  is cheap and stable on shared runners.
- Add a maintainer-facing diagnostics browser if quarantine and repair details
  become too important to leave in logs and smoke artifacts.
- Extend confined-runtime recovery smoke as Flatpak/Snap support matures.
