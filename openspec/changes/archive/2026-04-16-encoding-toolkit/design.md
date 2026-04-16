## Context

LushText's document pipeline is still centered on UTF-8 text snapshots:

- `services/editor_io.rs` reads bytes, validates UTF-8, and returns only a `String`, size data, and `mtime`.
- `ui/editor_page/load_save.rs` applies the decoded `String` directly to `GtkSourceBuffer`, and saves the buffer back as UTF-8 text via `write_snapshot_to_path`.
- `ui/status_bar` exposes a passive encoding label and file-size metadata, but no per-document encoding, line-ending, or file-health state.
- `docs/next/encoding-support.md` and `docs/next/encoding-toolkit.md` both point at the same missing foundation: raw-byte I/O, detected encoding state, and interactive status-bar controls.

The change is cross-cutting because it touches the I/O boundary, per-editor state, status-bar chrome, save safety, and future EditorConfig hooks. It also introduces external dependencies and new user-visible safety decisions around lossy conversions.

## Goals / Non-Goals

**Goals:**
- Add an encoding-aware load/save pipeline that preserves raw-byte facts such as detected charset, BOM presence, line-ending style, and decode confidence.
- Keep `EditorPage` as the per-document source of truth for encoding, line-ending, and file-health state, so window/status-bar refreshes do not re-derive metadata from widgets.
- Distinguish safe "reopen with encoding" flows from potentially destructive "save using encoding" flows.
- Give users clear status-bar entry points for encoding, line endings, and health details, with blocking confirmation only where data loss is possible.
- Add opt-in invisible-character modes that build on the same typed document metadata and remain compatible with GTK-native editing.

**Non-Goals:**
- Rewriting the editor around `GtkSourceFileLoader` / `GtkSourceFileSaver`.
- Building a full side-by-side diff viewer for the entire file; preview can stay bounded to changed regions.
- Enforcing `EditorConfig` `charset` or `end_of_line` on day one.
- Solving every binary-file detection or obscure legacy-encoding edge case in the first cut.
- Injecting synthetic text into the buffer to show invisibles; visualization must stay non-destructive.

## Decisions

### 1. Keep the existing `editor_io` service boundary and make it encoding-aware

The current `editor_io` split is already aligned with the repo's architecture: blocking filesystem and transcoding work stays off the GTK thread, while `EditorPage` applies results to widgets. This change should extend that boundary instead of replacing it with `GtkSourceFileLoader`.

The load path will evolve from `LoadResult { content, size, size_check, mtime }` into a richer document snapshot that also carries:

- the decoded Unicode text,
- detected/opened encoding,
- whether a BOM was present,
- detected line-ending style (`LF`, `CRLF`, `CR`, `Mixed`),
- file-health findings and decode confidence,
- any flags needed by save-time preview or warning flows.

The save path will similarly move from `write_snapshot_to_path(path, text)` to a service call that receives the buffer snapshot plus the document's chosen save encoding and line-ending policy, performs normalization/transcoding off the GTK thread, and returns updated size/mtime metadata.

Alternatives considered:
- **Switch to `GtkSourceFileLoader`/`GtkSourceFileSaver`**: attractive for built-in encoding support, but it would force a larger rewrite across async orchestration, large-file gating, and restore/load callbacks.
- **Keep UTF-8 service I/O and bolt transcoding into `EditorPage`**: rejected because it would mix blocking/transcoding logic into GTK code and duplicate decision points.

### 2. Model open-time interpretation separately from save-time output intent

Reopening a file with a different encoding is not the same action as saving future bytes in a different encoding. The design should keep those decisions separate in editor state so the UI can explain them clearly and avoid accidental data loss.

The document model should track at least:

- `opened_encoding`: how the current buffer text was derived from on-disk bytes,
- `save_encoding`: how the next save will encode the buffer,
- `line_ending_policy`: the line ending to write on save,
- `file_health`: derived warnings/details for the active buffer and source bytes.

`Reopen with Encoding...` rereads the on-disk bytes using a chosen decoder and replaces the buffer only after the normal unsaved-changes safety path says that is allowed. `Save using Encoding...` keeps the current Unicode buffer intact, previews any lossy transcoding, and only changes `save_encoding` after confirmation.

Alternatives considered:
- **One `encoding` field for both meanings**: simpler API, but it obscures whether the buffer was reinterpreted from disk or only scheduled for a different save format.
- **Immediate in-buffer conversion on encoding change**: rejected because the buffer is already Unicode; the destructive step is the next write, not the in-memory representation.

### 3. Treat line endings as I/O policy, not buffer state

GtkSourceView normalizes line endings to LF internally, so the buffer cannot remain the source of truth for original or chosen line-ending style. The load path should scan the raw bytes to determine `LF`, `CRLF`, `CR`, or `Mixed`, and the save path should rewrite line endings after buffer snapshotting but before encoding/output.

Mixed line endings should be represented explicitly in `file_health`, with a lightweight warning path that lets the user normalize to a chosen style without leaving the editor. The status bar should always show the currently selected write policy for file-backed tabs so users know what the next save will produce.

Alternatives considered:
- **Infer line endings from the live GTK buffer**: rejected because LF normalization loses the original signal.
- **Only normalize on explicit conversion commands and ignore original style**: rejected because users need to understand what they opened and what they will save.

### 4. Use status-bar controls plus targeted editor warnings instead of a separate settings-heavy tool window

The existing status bar already owns per-document metadata. Extending that cluster with compact controls keeps encoding work close to the active tab and matches the repo's preference for lightweight, editor-adjacent workflows.

The UI should add:

- an encoding control that exposes the current open/save encoding and actions to reopen or save using a different encoding,
- a line-ending control beside it,
- a conditional file-health indicator when actionable issues exist,
- editor-scoped warnings or confirmations for mixed line endings, low-confidence decode, or lossy save conversions.

This keeps "what state am I in?" in the status bar and "what must I decide right now?" in document-scoped warnings or dialogs.

Alternatives considered:
- **Dedicated preferences dialog or global encoding settings**: rejected because encoding choice is document-specific, not app-global.
- **One big modal toolbox for every action**: rejected because common inspection/conversion should be one click away from the current tab.

### 5. Stage invisible-character visualization with GTK-native primitives and explicit fallbacks

Whitespace-only visualization should lean on GtkSourceView's native space-drawing support so tabs, spaces, and trailing whitespace remain cheap and GTK-native. "All" mode can then layer in encoding-adjacent anomalies such as NBSP, BOM, or zero-width characters through editor-safe markers, annotations, or file-health affordances rather than requiring custom text rendering from day one.

This keeps the first implementation maintainable while still satisfying the core user need: making invisible problems discoverable without mutating the document.

Alternatives considered:
- **Custom inline glyph rendering for every invisible character immediately**: highest fidelity, but high complexity and uncertain interaction with GtkSourceView text layout and selection behavior.
- **Rely only on file-health reports and skip visualization**: rejected because the requested toolkit explicitly includes on-demand inspection in the editor workflow.

### 6. Prepare, but do not yet enforce, EditorConfig `charset` and `end_of_line`

`docs/next/editorconfig-future.md` already names `charset` and `end_of_line` as deferred because they depend on encoding-aware load/save and line-ending policy. This change should expose typed editor/document state that future EditorConfig resolution can consume, but it should stop short of enforcing those properties automatically in the first pass.

That sequencing keeps the change focused on interactive toolkit behavior while avoiding a second large feature branch inside the same proposal.

## Risks / Trade-offs

- [Encoding detection is heuristic for many legacy charsets] -> Use explicit confidence metadata, never hide the chosen decoder, and let users reopen with a different encoding.
- [Lossy save conversions can destroy user data] -> Preview changed/unrepresentable content before write, require explicit confirmation, and keep the current buffer as Unicode text.
- [Line-ending rewriting on large files can add save cost] -> Perform normalization in the existing background save pipeline and reuse large-buffer snapshot safeguards.
- [Advanced invisible-character rendering may exceed GtkSourceView's native drawer features] -> Ship whitespace-only mode on native primitives first and treat richer anomaly markers as a bounded extension.
- [The proposal combines foundational encoding support with broader UX work] -> Keep tasks phased so the raw-byte I/O and status-bar metadata foundation lands before preview, health, and visualization layers.

## Migration Plan

No persisted document migration is required. Existing files, drafts, and sessions remain valid because the buffer still holds decoded Unicode text; the change only adds richer metadata and a different save pipeline around that text.

The rollout order should be:

1. Add typed encoding/line-ending/file-health models and service-level load/save support.
2. Wire `EditorPage` and `refresh_status_bar()` to surface the new per-document state.
3. Add interactive reopen/save controls and line-ending normalization.
4. Add file-health details and invisible-character modes.
5. Follow with EditorConfig `charset` / `end_of_line` enforcement in a later change if the foundation proves stable.

Rollback is low risk because the data model stays in-memory and the app can fall back to UTF-8-only paths if the feature is gated off during development.

## Open Questions

- Which exact encoding shortlist should the first UI expose by default versus hide behind a fuller picker?
- Should low-confidence decode warnings appear immediately on open, or only inside the file-health surface unless the content contains replacement characters or other visible damage?
