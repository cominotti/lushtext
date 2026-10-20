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
| C1 | `document-scale-policy` | Removes the multi-second stalls on open and the A32 CRITICAL, using C0's measured thresholds as finalized by the idle re-confirmation of 2026-10-13 (`L_syntax` 2,000, `L_search_highlight` 10,000, `L_minimap` 22,000, `L_invisibles` 50,000, `N_visible` 500,000, `V_batch` 56,000). Also fixes the `\n`-only install slicer (pre-existing blocker). | Thresholds recorded in `long-line-documents.md` ("Idle re-confirmation (2026-10-13)"). **Implemented, in review** (branch `document-scale-policy`, stacked on PR #55); `S_syntax` (1 MiB applied) and `L_protect` (500, stall basis) remain open maintainer decisions |
| C2 | `long-line-protected-view` | The only way to make interaction inside giant paragraphs instant with stock GtkTextView (log attrs, IM and AT read the whole paragraph). | `L_protect` is not derivable from the frame half (see §3); C2 uses the stall basis (`L_protect_stall` 500) |
| C4 | `bound-window-construction-dispatch` | **Implemented (2026-10-15, PR #58 on `main`):** every GSettings write left window construction and interaction dispatches (construction 135–151 → 22–24 ms on a fresh keyfile profile, 77–91 → 22–24 ms steady). Not met: B_settle per construction dispatch (the 22–26 ms residual) and the keyfile flush (12.8–17.9 ms); see C4b and the dconf question in `long-line-documents.md` ("C4"). | — |
| C3 | `sliced-protected-install` | Conditional: adopt only if its own gate (the install case against C2) is over `B_settle`. C0 did not run the install group. | Its own first task |
| C5 | `explore-editing-long-paragraphs` | Optional evaluation: editing giant paragraphs needs segmentation or a custom widget. Only after C2, and only if the maintainer wants it. | — |

## 2. New candidates found by C0 (not yet proposed)

0. **C4b `first-frame-shell` (found by C4, 2026-10-15).** After C4 the
   constructing dispatch is GTK's `present` of the whole window (11.8–14.4 ms
   idle) plus template instantiation (8.5–11 ms), and the first paint is
   another ~15 ms dispatch; staging every hidden secondary surface saves only
   3–4 ms. Meeting B_settle per dispatch needs a minimal first-frame shell
   presented first and the editor shell built in later bounded dispatches.
   Needs a maintainer decision together with C4's dconf question (whether the
   Flatpak gains dconf access so the settings flush stops paying a 12.8–17.9 ms
   `fsync` on the GTK thread).

1. **Explain the A37 residual that reappeared once — done (2026-10-14,
   `explain-a37-residual`).** On 2026-10-13 the lane logged the fixed
   `GtkGizmo` snapshot warning once (find in a 15,000-character wrapped
   paragraph, beside heavy CPU load) and failed the run as designed. Cause: the
   end-of-document overscroll idle sets the view's bottom margin after each
   viewport height change (here the search bar's slide); that setter, like
   every default-style property setter, invalidates every line and queues no
   allocation, and a preempted main thread let the next frame — started ahead
   of the validation idle by GTK's synthesized post-relayout motion — redraw the
   view first. `frame_validation.rs` now queues the allocation on those
   properties' `notify`; reproduced deterministically and proved fixed
   (`long-line-documents.md`, "Idle re-confirmation (2026-10-13)"). A side
   observation for C1/C2: each such setter invalidates **every** line of the
   document, so the overscroll margin re-validates a large document after every
   viewport height change.
2. **C6 `bound-post-open-settle`.** Even a 400-byte `.txt` stalls 12–20 ms
   right after load readiness (first layout/paint plus post-publish
   projections); an HTML document pays a size-independent ~42 ms first
   highlight in a fresh process (36 ms idle; `L_syntax` now discounts it); the
   first wheel step on an unscrollable document waits 13–17 ms for the
   overlay-scrollbar indicator frame. These are why the baseline misses
   `B_settle` and the input-to-frame limit. **Idle verdict (2026-10-13):**
   `B_settle` indeterminate (worst 15.4 ms, bounds 15.1–17.4 ms; the loaded
   "over" was partly load), input-to-frame still over (24.5 ms, structural),
   `B_interact` within (5.6 ms). Gate unchanged: the baseline fits both on an
   idle reference run. Propose next, after C1 or in parallel with C4.
3. **Re-confirm the reference values on an idle machine — done
   (2026-10-13).** `L_search_highlight` rose to 10,000; `L_minimap` fell to
   22,000 (the loaded 50,000 was a derivation artifact); every other value
   held. Three derivation rules were corrected on the way (phase-by-phase
   differences, `L_syntax`'s first-highlight discount, `V_batch`'s dense
   reference floor). Record: `long-line-documents.md` §"Idle re-confirmation
   (2026-10-13)".
4. **The idle full-screen scroll floor (new, 2026-10-13).** On the idle
   reference machine every scroll dispatch over a full screen of text took
   5–7 ms (2–3 ms under background load), leaving almost none of
   `B_interact`; no length threshold removes it. Find out whether it is
   E-core power management (`powersave`/`balance_performance`, the lane's
   inherited E-core affinity) or the paint itself, and give it an owner (C6 or
   its own change). Bisecting `L_minimap` inside [22,000, 50,000) can ride
   along with the next reference run.
5. **A presentation-capable reference lane.** Headless Mutter with cairo
   double-buffers at 120 Hz, so the 2-frame input-to-frame limit sits on the
   lane's own floor (~16.7 ms) and `L_protect`'s frame half and `S_syntax`
   cannot be derived. Measuring on a real (or GL-presenting) session would turn
   those into real verdicts. Needs a maintainer decision because it touches the
   live-desktop rule (an isolated-XDG launch with consent, never the user's
   session state).
6. **Share the A37 frame-validation fix.** `ui/editor_page/frame_validation.rs`
   keeps line validation out of the snapshot only for the editor view. The
   same GTK behaviour (A37) applies to every `GtkTextView` with wide lines:
   Markdown preview, notes editors, local-history preview, file peek. Moving it
   into a GTK Lush crate and attaching it there is a platform-API decision
   (`gtk-lush-stewardship`).
7. **Lane posture: fail on unclassified warnings.** The lane fails on fixed
   defects that reappear (`FIXED_DEFECTS`) but only reports unclassified
   warnings. Making them fail would match the runtime-warnings rule; it changes
   the lane's documented verdict posture, so it is its own decision.
8. **Budgets in the evidence itself.** The lane reads the budget constants from
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
