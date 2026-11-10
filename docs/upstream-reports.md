# Upstream reports

This file records what LushText reported to, or follows in, GTK-family upstream
projects. Each entry says what we do instead while the problem stays open, and
what to change on our side once upstream resolves it. When you check on an
issue, update its row: the status and, if resolved, the fixed version. Local
drafts and reproducers live outside the repository, in
`~/Documents/Agentic/Reports/GTK/<folder>/`. The `upstream-gtk-reports` skill
drafts, triages and files new reports.

Last checked: 2026-11-10.

LushText ships the GNOME 51 runtime (GTK 4.24, GtkSourceView 5.22, Pango 1.58,
Libadwaita 1.10) and keeps GNOME 50 (GTK 4.22, GtkSourceView 5.20, Pango 1.57)
as the source floor for the Snap and distribution builds (`adopt-gnome-51`).
One fix recorded below reached the shipped runtime: the `GtkSourceMap`
detach CRITICAL (A32) is fixed in GtkSourceView 5.22.0, so the shipped Flatpak
no longer needs its workaround, but the floor does, and the workaround (A45's
retire-and-rebuild) stays. None of the other upstream fixes has reached the
GNOME 50 or GNOME 51 runtimes, so every other LushText workaround stays until
the fixed versions are the floor. The GNOME
nightly re-test the maintainers asked for is queued after `adopt-gnome-51`
(`docs/next/long-line-documents.md`).

## Filed by us

| Filed | Issue | Status | What it is | Our workaround | When resolved |
|---|---|---|---|---|---|
| 2026-11-04 | [GNOME/gtk#8528](https://gitlab.gnome.org/GNOME/gtk/-/work_items/8528) | open; [GNOME/gtk!10651](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/10651) "Reduce chances GtkTextView will cause long stalls" merged on main 2026-11-05, and [!10228](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/10228) "textiter: stop visible line index scan at line boundary" is open; the maintainer asked us to re-test with GTK and Pango main (pending, after `adopt-gnome-51`). Not in the GNOME 50 or GNOME 51 runtimes (GTK 4.22/4.24), so our workaround stays until a fixed version is the floor. | `gtk_text_view_get_iter_at_location()` below the text walks every invisible byte of the last line (44/88/177 ms per lookup at 1/2/4 MiB) | Protected View folds an undelimited last paragraph behind a stand-in `"\n"` terminator (`ui::protected_fold::document_end_iter`, axiom A79) | Drop the stand-in terminator and its exclusions in `buffer_snapshot.rs` and the properties panel, once the fixed GTK is the floor |
| 2026-11-04 | [GNOME/gtk#8529](https://gitlab.gnome.org/GNOME/gtk/-/work_items/8529) | open; the maintainer (chergert) says the `chars_changed_stamp` cost is addressed by [GNOME/gtk!10646](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/10646), merged 2026-11-05 on main only (not in `gtk-4-24`). Not in the GNOME 50 or GNOME 51 runtimes (GTK 4.22/4.24), so our workaround stays until a fixed version is the floor. | The `retrieve-surrounding` answer for input methods is the whole paragraph, so typing in a long paragraph gets slow | LushText bounds the surrounding text itself (C5, axiom A54) | Re-measure C5's keystroke exception and drop our bound if GTK's is enough |
| 2026-11-04 | [GNOME/gtksourceview#432](https://gitlab.gnome.org/GNOME/gtksourceview/-/work_items/432) | open; [re-measured on master](https://gitlab.gnome.org/GNOME/gtksourceview/-/issues/432#note_2922671) 00a07c1 (after !463): unchanged. Then [!465](https://gitlab.gnome.org/GNOME/gtksourceview/-/merge_requests/465) "prevalidate syntax files at compile time" merged on master 2026-11-04 (the maintainer measured about 19/16/11 ms for html/markdown/json) and the maintainer says that is all he can commit to; still over `B_SETTLE_US` for html, so the exception stays even once it ships. Not in the GNOME 50 or GNOME 51 runtimes (GtkSourceView 5.20/5.22), so our workaround stays until a fixed version is the floor. | The first `set_language()` for HTML, Markdown or JSON parses the `.lang` files on the main loop (20–35 ms), with no way to prepare it ahead of time | Recorded toolkit-floor exception (C6, axiom A52); HTML and JSON stay gated by size | Lift the A52 exception and re-measure C6/C6b's first-attach cost |
| 2026-11-04 | [World/Rust/sourceview5-rs#19](https://gitlab.gnome.org/World/Rust/sourceview5-rs/-/work_items/19) | fixed on main by [!42](https://gitlab.gnome.org/World/Rust/sourceview5-rs/-/merge_requests/42) (2026-11-04): `replace_all()` now returns `Result<u32, glib::Error>`; still unreleased on 2026-11-09 (latest crate 0.11.2). Not in any crate LushText can use yet, so the workaround stays. | `SearchContext::replace_all()` panics when nothing matches (it asserts that a zero count carries an error) | The search bar calls `gtk_source_search_context_replace_all` directly in one `unsafe` block (`ui/search_bar/execution.rs`) | Go back to the safe binding (note the new `u32` count) once a `sourceview5` release with !42 is out |
| 2026-11-04 | [gtk-rs/gtk4-rs#2391](https://github.com/gtk-rs/gtk4-rs/issues/2391) | open; our fix [gtk-rs/gtk4-rs#2398](https://github.com/gtk-rs/gtk4-rs/pull/2398) opened 2026-11-09, not merged. Not in any released `gtk4` crate, so the workaround stays. | The `IMContextImpl` `get_preedit_string` trampoline writes to NULL out-parameters and crashes | The responsiveness probe's input method restores GTK's own `get_preedit_string` (`ui/responsiveness_probe.rs`) | Remove the class-vfunc override after bumping `gtk4` |

## Comments we added to existing issues

| Date | Issue | Status | What we added | Our workaround | When resolved |
|---|---|---|---|---|---|
| 2026-11-04 | [GNOME/gtk#6057](https://gitlab.gnome.org/GNOME/gtk/-/issues/6057#note_2922623) | open (since 2023) | The default-style setters (margins, tabs, wrap mode…) invalidate every line and queue no allocation, so validation is flushed inside the snapshot (`Trying to snapshot GtkGizmo … without a current allocation`); a 12-frame margin repro, and `gtk_widget_queue_allocate` as the cure | `ui/editor_page/frame_validation.rs` queues the view's allocation after edits, caret moves, tag changes and default-style changes (axiom A37) | Keep `frame_validation.rs` until the fixed GTK is the floor, then retire it with the A37 probe |
| 2026-11-04 | [GNOME/pango#582](https://gitlab.gnome.org/GNOME/pango/-/issues/582#note_2922624) | open (since 2020); the maintainer says commit `f60f461` on main (in no tag yet) likely fixes it and asked us to re-test (pending, after `adopt-gnome-51`). Not in the GNOME 50 or GNOME 51 runtimes (Pango 1.57/1.58), so our workaround stays until a fixed version is the floor. | Root cause: x positions are `int` Pango units and wrap negative past ~2.1M px (209,716 characters at a 10 px advance), and the logical width comes back 0 | Wrap-off paragraphs past the overflow length count as unrenderable in the document scale policy (C1, axiom A28) | Unlikely in Pango 1.x (ABI); revisit if Pango clamps or GTK moves to a wider unit |

## Drafted, not filed

Drafts waiting for the maintainer's per-report approval; nothing here has been
filed.

| Drafted | Project | What it is | Our workaround | Draft |
|---|---|---|---|---|
| 2026-11-10 | GNOME/gtksourceview (5.22.0 and master) | Since 5.22.0 (`8d679ccd`) the map skips allocating its slider while the slider rectangle is unchanged, even when the slider queued a resize; a CSS change of the slider's size then leaves it without a current allocation and every drawn frame logs `Trying to snapshot GtkSourceMapSlider … without a current allocation` (axiom A103). Not in any release; 5.20 is unaffected | The minimap applies its one-pixel wide-editor slider correction class only below GtkSourceView 5.22 (`adopt-gnome-51` W1) | `~/Documents/Agentic/Reports/GTK/gtksourceview-map-slider-unallocated/draft.md` |

GtkSourceView 5.22's slider geometry (A106: the slider follows each view's
content height, margins included, so a map must carry the view's margins scaled
by its text scale), GTK's own changes behind W2 (A104, `GtkListView`
height-for-width, `!9906`)
and L1 (A105, the grid-aligned frame time of the reworked frame clock) are
intended behaviour, not defects: LushText adapts (the slice bin measures its
child's width for no height; the responsiveness probe stamps each frame's
start; the minimap scales the overscroll margin it gives its map) and reports
nothing.

## Followed, not filed

These were triaged on 2026-11-04 and are not worth a new report, but their
outcome still matters to us.

| Topic | Upstream state | Why not filed | What to do |
|---|---|---|---|
| `GtkSourceMap` update tick survives `set_view(NULL)` (A32, A45) | Fixed on master and in 5.22.0 (commit [`8d679ccd`](https://gitlab.gnome.org/GNOME/gtksourceview/-/commit/8d679ccd4143031e7f55efcc76b8af20b43e0ad4)), so **in the shipped GNOME 51 runtime**; not on the dormant 5-20 branch (the GNOME 50 floor). A32 is now version-qualified and fails if the defect returns on 5.22 | A backport request to a dormant branch adds little | Keep retiring the map (A45) while 5.20 is supported (it is also correct on 5.22, `adopt-gnome-51` D5); drop the floor half once 5.22 is the floor |
| Context engine CRITICAL after the per-line time-out (A29) | Fixed on master (commit [`aa6a36a3`](https://gitlab.gnome.org/GNOME/gtksourceview/-/commit/aa6a36a3), 2026-11-04); the 5-22 branch still lacks the guard, and the maintainer is backporting the series | Too early: it landed the same day | Re-check whether 5.22.x picks it up; then update A29 |
| Context engine background analysis runs in fixed 10/30 ms idle slices (A54, C6 D4) | The slice constants are unchanged on master, but [MR !463](https://gitlab.gnome.org/GNOME/gtksourceview/-/merge_requests/463) ("Reduce repeated work in context engine") reworked that code and merged on 2026-11-04 | Too early to report against code that just changed | Re-measure the slices on the next release with !463 (A54); LushText paces around them meanwhile (C6, C6b) |
| Search copies the line per match on long lines (A33) | The copy is in GTK's `gtk_text_iter_forward_search`, not in GtkSourceView | Wrong project for the candidate, and a borderline performance report | Triage again as a GTK report if long-line search becomes a goal |
| The commit handler lays out the whole paragraph on every keystroke (C5) | GtkTextView design (one PangoLayout per paragraph); the related [GNOME/gtk#5764](https://gitlab.gnome.org/GNOME/gtk/-/work_items/5764) (cursor movement in long paragraphs) was worked on and closed on 2023-09-14 | Design, not a defect | Nothing to follow; C5's exception stays recorded |
| The IM spot location is updated on every frame of an animated scroll (A30), and log attrs are computed over invisible text (A26) | Not triaged yet | — | Triage with the `upstream-gtk-reports` skill when convenient |
