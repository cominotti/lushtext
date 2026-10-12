# Long-line responsiveness — natural next candidates

> Planned-work record, written when C0 (`probe-long-line-toolkit-costs`) was
> archived (2026-10-12). It lists where this work naturally goes next, in
> priority order, so a later session does not have to re-derive it from the
> C0 record ([`long-line-documents.md`](./long-line-documents.md)) and the
> origin proposal ([`long-line-responsiveness.md`](./long-line-responsiveness.md)).
> Update it when a candidate is proposed, done, or dropped.

The requirement behind all of it: LushText stays instantly responsive (open,
scroll, every interaction) with any document, with `B_interact = 8 ms` and
`B_settle = 16 ms` per main-loop stall and 2 reference frames (120 Hz) from
input to presented frame, measured on the maintainer's desktop.

## 1. Already proposed — implement in this order

| # | Change | Why it is next | Gate from C0 |
|---|---|---|---|
| C1 | `document-scale-policy` | Removes the multi-second stalls on open and the A32 CRITICAL, using C0's measured thresholds (`L_syntax` 2,000, `L_search_highlight` 6,700, `L_minimap`/`L_invisibles` 50,000, `V_batch` 56,000). Also fixes the `\n`-only install slicer (pre-existing blocker). | Thresholds recorded in `long-line-documents.md` |
| C2 | `long-line-protected-view` | The only way to make interaction inside giant paragraphs instant with stock GtkTextView (log attrs, IM and AT read the whole paragraph). | `L_protect` is not derivable from the frame half (see §3); C2 uses the stall basis (`L_protect_stall` 500) |
| C4 | `bound-window-construction-dispatch` | Window construction stalls 150–196 ms to first frame on **every** open, short files included — the largest budget violation C0 measured. Independent of C1/C2 code; can run in parallel with them. | — |
| C3 | `sliced-protected-install` | Conditional: adopt only if its own gate (the install case against C2) is over `B_settle`. C0 did not run the install group. | Its own first task |
| C5 | `explore-editing-long-paragraphs` | Optional evaluation: editing giant paragraphs needs segmentation or a custom widget. Only after C2, and only if the maintainer wants it. | — |

## 2. New candidates found by C0 (not yet proposed)

1. **C6 `bound-post-open-settle`.** Even a 400-byte `.txt` stalls 12–20 ms
   right after load readiness (first layout/paint plus post-publish
   projections); an HTML document pays a size-independent ~42 ms first
   highlight in a fresh process; the first wheel step on an unscrollable
   document waits 13–17 ms for the overlay-scrollbar indicator frame. These are
   why the baseline is over `B_settle` and the input-to-frame limit. Gate: the
   baseline fits both on an idle reference run. Propose next, after C1 or in
   parallel with C4.
2. **Re-confirm the reference values on an idle machine.** The reference run
   shared the machine with a game and a VM, so every derived value is a lower
   bound, and three thresholds (`L_search_highlight`, `L_protect_stall`,
   `S_syntax_interact`) plus the `B_settle` verdict are load-sensitive. One
   resumable command (~30 min) is in `long-line-documents.md` §"Re-confirm on
   an idle machine". Do it before C1 freezes its constants.
3. **A presentation-capable reference lane.** Headless Mutter with cairo
   double-buffers at 120 Hz, so the 2-frame input-to-frame limit sits on the
   lane's own floor (~16.7 ms) and `L_protect`'s frame half and `S_syntax`
   cannot be derived. Measuring on a real (or GL-presenting) session would turn
   those into real verdicts. Needs a maintainer decision because it touches the
   live-desktop rule (an isolated-XDG launch with consent, never the user's
   session state).
4. **Share the A37 frame-validation fix.** `ui/editor_page/frame_validation.rs`
   keeps line validation out of the snapshot only for the editor view. The
   same GTK behaviour (A37) applies to every `GtkTextView` with wide lines:
   Markdown preview, notes editors, local-history preview, file peek. Moving it
   into a GTK Lush crate and attaching it there is a platform-API decision
   (`gtk-lush-stewardship`).
5. **Lane posture: fail on unclassified warnings.** The lane fails on fixed
   defects that reappear (`FIXED_DEFECTS`) but only reports unclassified
   warnings. Making them fail would match the runtime-warnings rule; it changes
   the lane's documented verdict posture, so it is its own decision.
6. **Budgets in the evidence itself.** The lane reads the budget constants from
   Rust source with a fallback; emitting them in the probe's `start` record
   (and computing verdicts against those) removes that coupling. A schema
   change to the JSONL evidence.

## 3. Upstream reports (file when convenient; none blocks the plan)

- GtkSourceView: `GtkSourceMap` keeps `update_id` after `set_view(NULL)`
  (A32); search copies the line per match (A33).
- GTK: `GtkTextView` validates inside the snapshot after in-frame changes
  (A37); log attrs include invisible text (A26); the IM spot is updated on
  every scroll (A30).
- Pango: int32 glyph-width overflow above ~238k monospace characters (A28).
- gtk4-rs: the `IMContextImpl` `get_preedit_string` trampoline writes NULL
  out-parameters (worked around in `ui/responsiveness_probe.rs`).

## 4. Outside the long-line stream, but still queued

- Formal verification: `apply-verification-altitude-redesigns`,
  `verify-shell-conformance-against-proven-cores`,
  `evaluate-verus-for-unbounded-proofs`.
- `stop-expander-bind-time-child-scans` (the sidebar scans every visible,
  never-expanded directory; found through axiom A22).
- Host: during the final quick run, `btrfs-stall-report` flagged
  `fstrim.service` blocked 35 s in a disk-discard wait. Not caused by the
  lane; a storage alert for the maintainer to look at.
