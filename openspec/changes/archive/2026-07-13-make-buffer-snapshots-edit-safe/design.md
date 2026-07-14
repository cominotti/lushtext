## Context

`ui/buffer_snapshot.rs` provides direct and chunked text capture to draft, save, encoding, note-preview, and local-history workflows. The chunked functions currently move a `GtkTextIter` into a later timeout. GTK invalidates outstanding iterators when character-count-changing edits occur; `GtkTextMark` is the stable-position mechanism. Some consumers generation-check only after a complete snapshot, which is too late to prevent invalid iterator reuse. Periodic history separately uses a generation counter around raw five-minute sources but cannot remove superseded `SourceId`s.

This is GTK adapter infrastructure, not a service or domain abstraction. It must keep GTK objects on the main thread, allow background consumers to receive only owned text, preserve small-buffer performance, and expose lifecycle completion exactly once.

## Goals / Non-Goals

**Goals:**

- Make cross-turn positions valid under GTK's buffer mutation contract.
- Cancel on source mutation before the next position is resolved or text is copied.
- Give every consumer an explicit terminal outcome and cleanup path.
- Own temporary marks, signal handlers, and timer sources in one snapshot session.
- Replace periodic-history raw timer accumulation with tab-owned superseding scheduling.

**Non-Goals:**

- Moving buffer snapshots into GTK Lush or services.
- Snapshotting GTK buffers on worker threads.
- Changing draft size limits, save encoding semantics, or note-preview rendering.
- Providing a general reactive-stream abstraction.

## Decisions

### 1. Use a session object with `GtkTextMark` cursor ownership

A private snapshot session owns the source buffer, a left-gravity progress mark, output string, byte policy, cancellation state, change-handler ID, scheduled source ID, and one terminal callback. Each slice resolves a fresh iterator from the mark, advances a local iterator, copies the chunk, and moves the mark before yielding.

All terminal paths funnel through one `finish` routine that removes the scheduled source, disconnects the handler, deletes the mark, releases owned text as appropriate, and consumes the callback exactly once.

**Alternative considered:** store integer character offsets. Rejected because edits before the offset alter meaning and still require mutation detection; marks directly encode GTK's supported persistent-position contract.

### 2. Cancel synchronously from the buffer `changed` signal

The session's tracked `changed` handler marks cancellation before GTK returns to the main loop. The next slice checks cancellation before resolving its mark or reading text. The handler does not copy text, schedule I/O, or invoke workflow code; terminal callback delivery stays in the scheduled slice/idle path to avoid reentrant consumer mutation inside `changed`.

**Alternative considered:** generation-check only after capture. Rejected because invalid iterators can already have been used and the captured string may mix generations.

### 3. Converge private APIs on typed terminal outcomes

Use a common private outcome that distinguishes complete capture, cancellation/source mutation, and byte overflow. Consumers map that outcome deliberately:

- save freezes editing and always restores interactivity on non-success;
- draft autosave keeps dirty state and coalesces one later attempt;
- periodic history releases its permit and reschedules only if still eligible;
- encoding/note preview discard stale work and let existing generation/debounce policy drive the latest request;
- local-history restore never applies a stale undo snapshot.

Direct small-buffer capture may continue synchronously but returns the same semantic outcome where a byte budget applies.

### 4. Use `SupersedingTimer` for periodic history

Replace raw timeout generation-only cancellation with the existing tab-owned GTK Lush settle primitive. The timer is cancelled on clean transition, path change, ineligibility, and disposal; scheduling a later deadline replaces the earlier source. Capture-generation checks remain because a timer primitive does not replace workflow freshness.

**Alternative considered:** store and manually remove `SourceId`. Acceptable but rejected in favor of the governed existing primitive that already encodes superseding cleanup.

### 5. Keep the helper app-local

The snapshot session remains under LushText UI because its consumer policy, buffer threshold, byte outcomes, and editor workflows are app-specific. A future GTK Lush extraction would require independent second-consumer evidence and stewardship review.

## Risks / Trade-offs

- **[Risk] Deleting a mark or disconnecting a signal twice causes GTK warnings.** → Make cleanup idempotent and exercise success, cancellation, overflow, and disposal separately.
- **[Risk] Immediate changed-signal callback reentrancy mutates workflow state.** → The handler only records cancellation; terminal delivery remains scheduled.
- **[Risk] Save can become editable before snapshot cleanup.** → Restore interactivity only from the one terminal consumer path after session cleanup.
- **[Risk] A mutation between final slice copy and terminal completion is accepted.** → Keep the handler connected through final validation and disconnect only inside `finish` after confirming no cancellation.
- **[Trade-off] Marks and a signal add small per-snapshot GTK overhead.** → Use them only for chunked captures; the direct small-buffer path remains unchanged.

## Migration Plan

1. Add session-level tests and the new outcome API beside existing helpers.
2. Migrate cancellable draft and periodic-history consumers first.
3. Migrate save, encoding, note preview, and local-history restore with consumer-specific tests.
4. Remove iterator-carrying helpers and raw periodic-history timeout state.
5. Run widget lifecycle, runtime warning, responsiveness, full test, and strict OpenSpec gates.

No persisted data or public application API changes; rollback is a source-level revert.

## Open Questions

- None. Official GTK guidance fixes the persistent-position choice; only private naming remains implementation-local.

## Toolkit Contract Verification

Verified against GTK 4.22.4's official `gtktextiter.c`, `gtktextmark.c`, and
`gtktextbuffer.c` sources plus the GNOME [text widget overview](https://docs.gtk.org/gtk4/section-text-widget.html)
and [`GtkTextMark` documentation](https://docs.gtk.org/gtk4/class.TextMark.html):

- any mutation that changes indexable buffer contents invalidates all outstanding
  `GtkTextIter` values, even if a later edit restores the same character count;
- `GtkTextMark` preserves a logical position across insertion and deletion, and
  a left-gravity mark stays before text inserted exactly at its position;
- an anonymous mark must be removed from its buffer with `delete_mark`; retaining
  a Rust/GObject reference does not keep a deleted mark useful or remove a live
  mark from the buffer;
- each slice must obtain a fresh iterator with `iter_at_mark`, use it only during
  that slice, and move the mark before yielding.

The resolved checkout uses `gtk4` 0.11.3 and `glib` 0.22.7. The available Rust
bindings are `TextBufferExt::create_mark`, `iter_at_mark`, `move_mark`,
`delete_mark`, and `connect_changed`, `ObjectExt::disconnect` for the returned
`SignalHandlerId`, and consuming `SourceId::remove` for queued GLib sources.

## Baseline Consumer Inventory

| Consumer | Capture/editability | Freshness and terminal policy | Disposal/retry owner |
| --- | --- | --- | --- |
| Editor save | Chunked above the shared threshold; source view is frozen | Only `Captured` reaches encoding/write work; cancellation restores the prior editable/cursor state and leaves modified content | Editor `SaveState`; disposal is silent, the caller receives one save error while alive |
| Draft autosave | Direct or byte-budgeted chunked while editing remains enabled | Draft ID and dirty generation gate acceptance; mutation/cancellation retains dirty state and coalesces one latest pass | Window draft pipeline; window disposal removes the active session |
| Close draft flush | Direct or byte-budgeted chunked while close is pending | Any mutation/cancellation is unconfirmed and blocks close; overflow/body/manifest failures remain explicit | Window close pipeline; window disposal removes the active session |
| Periodic local history | Direct or byte-budgeted chunked while editing remains enabled | Editor/path/timer/edit generations and availability gate persistence; cancellation drops the permit and schedules only current eligible work | Editor local-history state owns one snapshot and one superseding timer |
| Lossy-encoding analysis | Direct or chunked while editing remains enabled | Analysis/content generations and active-editor identity discard stale results; cancellation publishes nothing | Editor metadata state supersedes/disposes the active capture |
| Local-history restore safety | Direct or chunked while editing remains enabled | Only a coherent undo snapshot may reach safety persistence and restore; cancellation leaves the browser and editor unchanged | Local-history browser/dialog owns and disposes the capture |
| Note Save sensitivity | Direct or chunked while editing remains enabled | Debounce plus single-flight/rerun state applies only a complete latest body | Dialog refresh state disposes the capture on close |
| Note Render preview | Direct or chunked while editing remains enabled | Mutation discards the render and the next visible-mode/edit request supplies current text | Preview widget owns and disposes the source capture |
| Document/folder note save | Direct or chunked after the dialog response | Only complete text reaches sidecar persistence; cancellation reports that preparation changed | The window owns active captures through disposal; terminal callbacks prune the set and use weak UI ownership |
| Markdown document preview | Direct only below the threshold | Large documents show the existing paused placeholder; debounce owns freshness | No cross-turn snapshot resource |
| Minimap warning/layout scans | Direct only after explicit size guards | Large buffers skip the scan | No cross-turn snapshot resource |

The existing `gtk_lush_settle::SupersedingTimer` API remains unchanged. Its
implementation now removes the previous GLib source on re-arm/invalidation so a
five-minute periodic-history deadline cannot accumulate obsolete sources.
