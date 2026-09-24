## 1. Reproduce

- [x] 1.1 Drive the real sidebar under the widget harness and record the settled outer scroll position, the header's bounds, and the behaviour of a scroll back to the top
- [x] 1.2 Record the two-workspace case and confirm the oscillation and the unreachable end

## 2. Fix the request decision

- [x] 2.1 Add `crates/gtk-lush/widgets/src/scroll_request.rs` with the pure `outer_scroll_request` and its unit tests, and export it
- [x] 2.2 Record the published slice offset in `ViewportSliceBin` and route both the in-allocation and out-of-allocation paths through the pure decision

## 3. Fix the band the child is given

- [x] 3.1 Deduct chrome above the content from `viewport_slice`, leaving the bottom edge alone, and update its unit tests
- [x] 3.2 Confirm the bottom edge and the scrolled-past case still yield a full, non-zero band

## 4. Regression coverage

- [x] 4.1 Property tests for the resting invariant, the landing position, settling after an honoured request, and the epsilon bound
- [x] 4.2 Generic widget coverage in `gtk_lush_adoption.rs`: resting, return to top, two bins without oscillation, and a positive control that a real `scroll_to` is still forwarded and still settles
- [x] 4.3 Sidebar coverage in `workspace_tree_virtualization.rs`: header visible at rest, return to top, after expanding, after a manual refresh, after collapse/expand, after toggling hidden files, in a short window, and two sections reaching both ends
- [x] 4.4 Guard every resting assertion with a scroll-range precondition so none can become vacuous
- [x] 4.5 Restate the bottom-tiling assertion so it forbids rows drawn below the fold instead of accepting a recycled out-of-range widget
- [x] 4.6 Prove each new check fails against the defect, both halves independently

## 5. Documentation and gates

- [x] 5.1 Correct the `widget-wiring.md` bullet that prescribed the defective comparison and add the band-deduction rule
- [x] 5.2 Update the widget crate CHANGELOG, the slice geometry module docs, and the AGENTS.md viewport-slice decision
- [x] 5.3 Delta spec for `gtk-lush-viewport-slice`
- [x] 5.4 Run `make check`, the property lane, the full widget suite, the GTK Lush lanes, and the visual/accessibility smoke lanes
- [x] 5.5 Exercise the fix in a live session (`make run`) under the debug-session recorder, per the live-acceptance rule for sidebar geometry. The workspace header's collapse button, label, and refresh button all report AT-SPI `showing, visible` at rest with the tree overflowing (15 of 37 rows rendered, so the check is not vacuous), a screenshot confirms the header bar above the file tree, and the whole session's stderr holds zero `Gtk-`/`Gdk-`/`Adwaita-`/`GLib-GObject-` warnings, `pixman` errors, or `Trying to measure` lines. The dev data directory's workspace had no folders, so it was pointed at 37 repository directories to produce the geometry and restored afterwards from a backup
