## Why

Text typed into a document while a workflow owns its buffer is lost. A load installation, a
whole-buffer replacement (draft recovery, a local-history restore or its undo, save formatting) and
a save hold C6's buffer-level edit lock (`ui/editor_page/edit_lock.rs`), which stops every
`insert-text` that is not the owning workflow's own. Typed characters are refused: they never reach
the buffer, and nothing tells the user. Before C6 the view was `editable=false` for the same
stretch, which dropped the same keys. PR #76's delivery gate exposed it: under load,
`type-during-settle` on `total-size/1000000` and `total-size/5000000` delivers 60 keys to the editor
and inserts 0 to 3 of them, on `main` and on every branch alike.

There is a second route to the same loss that the lock does not cover. While a load is still being
read and planned on its worker (`load_state == Loading`, before the install takes the lock), the view
is editable and the buffer accepts typing. The install then clears the buffer and installs the file,
so the typed text is deleted again, without a word. The accessible state already reports the editor
read-only while it loads.

The programme's goal for this change: **no typed character is ever silently lost.**

## What Changes

- **Typed text is held, then inserted (design D1, option (a)).** While a workflow holds the edit lock
  or a load is in flight, an insertion the user types (`insert-text` inside a user action, at the
  caret, in a user action that deleted nothing) is not refused but held, in order, in a bounded
  per-editor buffer. When no workflow holds the buffer any more, one dispatch inserts the held text at
  the caret as one user action: one undo step, through the ordinary insert path, so the modified
  state, drafts, the document-scale watch and every other edit handler see an ordinary edit. Keys
  typed after the release and before that dispatch are held too, so the order is kept.
- **Backspace edits the held text.** Backspace while text is held removes the last held character.
- **Everything else is refused with feedback, as before, but no longer silently.** A deletion of
  document text, undo, redo, a paste or insert away from the caret, an edit that also deleted (typing
  over a selection, search Replace, moving lines), and an insert that would overflow the held buffer
  are refused, and a rate-limited status warning says so.
- **Where the held text goes.** It belongs to the editor it was typed into and is inserted only into
  a whole document: a completed load, replacement or save, or a reload that failed and kept the
  previous text. When the episode ends with the buffer partial or emptied (a cancelled load, a
  cancelled replacement that had cleared the buffer, a failed first load, an evicted tab), the held
  text is discarded with a status warning that names how many characters were not inserted. When the
  document ends up in Protected View (C2), it is discarded with a Protected View notice.
- **Feedback while held.** The first held character of an episode publishes one status message
  saying the typed text will appear when the document is ready, and the editor's accessible
  description says so while text is held.
- **Bounded.** The held buffer is a power-of-two cap of 4 KiB of UTF-8 (`HELD_TEXT_CAP_BYTES`); an
  insertion that does not fit whole is refused whole, never split.
- **Lane.** The measurement probe classifies keys that arrive while the editor holds input as
  `held-editor`; the driver counts them as keys into an editor, so the delivery gate requires their
  insertion, and reports their input-to-frame apart, like Protected View keys. The probe's
  `editor_user_insert` mark counts characters, as the gate's specification already says.

## Capabilities

### New Capabilities

- `held-typing`: what happens to text typed while a workflow owns the document's buffer.

### Modified Capabilities

- `responsiveness-measurement`: keys typed while the editor holds input are reported and gated.

## Impact

- `crates/lushtext-core/src/model/held_input.rs` (new, pure policy: the held buffer and its cap, the
  insert and settle decisions).
- `crates/lushtext-core/src/ui/editor_page/held_input.rs` (new, cross-cutting, no role: the capture,
  the settle dispatch, the replay, the notices and the evidence).
- `ui/editor_page/edit_lock.rs` (the lock's handlers call the capture; `unlock_edits` takes the
  release's content disposition), its callers in `load/execution.rs`, `load/retirement.rs`,
  `buffer_replacement/execution.rs` and `save/execution.rs`, `accessibility.rs`,
  `protected_view/policy.rs` (one notice), `surrounding_text.rs` (the input-method context lookup is
  shared).
- Measurement build: `ui/responsiveness_probe.rs`, `services/responsiveness_log.rs`,
  `scripts/responsiveness-smoke-driver.py`.
- Docs: `AGENTS.md`, `crates/lushtext-core/src/ui/editor_page/AGENTS.md`, `docs/accessibility.md`,
  `docs/accessibility-matrix.md`, `docs/next/long-line-documents.md`, `.agents/rules/build.md`.
- No persisted format, action, D-Bus member or snapshot field changes.
