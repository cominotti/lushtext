## 1. Axiom first (A86)

- [ ] 1.1 A86 probe in `gtk-lush-axioms` (from the design's baseline probe): reload with unchanged text, reload zoom, per-widget class swap, ancestor and window class controls, at 0 and 1,500 unrelated rows with and without the dense view; the Pango size check on the view and labels; the unmatched-rules load (rules keyed by classes no widget carries: revalidation, no font or layout change, D4); the fixed-pitch control step. Commit the probe from the design's baseline source (the scratchpad copy is not durable)
- [ ] 1.2 A86 sample with an interactive mode (zoom buttons for each mechanism, the last frame cost shown) and `--check`; catalogue entry; ledger row with "Verified against" and "Sample"
- [ ] 1.3 `make gtk-axioms` and `make check-gtk-axioms` green; record the probe's figures in the programme record

## 2. Pure rule text

- [ ] 2.1 A pure function producing the provider text from (generation, family, base points, zoom set, system-font flag), and the zoom set from the current level (grid plus its mod-10 lattice, at most 72 levels), in `ui/font_zoom.rs`'s GTK-free half (or `model/`, if a second consumer appears); unit tests that each zoom class reproduces `apply_font_css`'s current rules for that level, including the system-font-at-100 % omission, an off-grid level and its lattice (55 → 65 → 75 needs no reload)
- [ ] 2.2 Failing first: a widget test that a zoom step leaves the font provider's text unchanged (fails on `main`, which reloads)

## 3. Provider and registry

- [ ] 3.1 `ui/font_zoom.rs`: the current class, `follow_editor_font` (adds the class, holds a weak reference), `apply_zoom` (swaps classes, prunes), the amortized prune; evidence (registered, live, swaps, last swap µs, provider loads, current generation) for tests
- [ ] 3.2 `ui/theme.rs`: two font providers and a generation (D4); load the idle provider per base font (startup, custom font, `use-system-font`, desktop `monospace-font-name`) and per new zoom lattice, then swap the generation's classes after the next paint inside `restyle_everywhere`; keep the zoom handler on a known lattice to a class swap inside `restyle_everywhere`; a second change before the swap supersedes it (one pending swap, latest wins)
- [ ] 3.3 Register the editor surface `source_view` and every `.monospace` site: search-result labels, file peek path label and text view, minimap, Markdown code blocks and image fallback bodies, notes raw preview, local-history preview, Line Inspector
- [ ] 3.4 Measure that the larger rule set does not slow an unrelated restyle (redraw and `:hover` change on the populated window), before and after; if it does, apply the risk's fallback grid and record it

## 4. Tests and policy

- [ ] 4.1 Widget test: a zoom step on a window with many unrelated rows changes no provider and swaps exactly the registered widgets' classes
- [ ] 4.2 Widget test: a widget created after a zoom step (new tab, search row, preview code block, peek) carries the current class and the zoomed Pango size
- [ ] 4.3 Widget tests per surface: computed font family and size (and the editor's line height) equal `main`'s rules at 100 % system font, 100 % custom font, 130 % custom font and 50 %, and at 130 % under a 1.5 text scale (`gtk-xft-dpi`) and with high contrast; the Focus Mode readable-column margins at 100 % and 130 % equal `main`'s; failing first where a surface is not yet registered
- [ ] 4.4 Policy check (in `make check-policy`, comment-stripped): `add_css_class("monospace")`, `set_monospace(true)`, `css_classes([.."monospace"..])`, Blueprint `monospace: true` and Blueprint `styles ["monospace"]` are forbidden in `crates/lushtext-core/src` and `resources/ui` except inside `ui/font_zoom.rs`'s registering helper; each Blueprint site is listed in the script with the Rust construction site that registers it, and the script checks that site calls `follow_editor_font` on that template child; the message names the call site; self-test with a failing and a passing case. Plus a widget test that walks every widget of every open window and dialog in a populated session (search results, peek, preview with code block and image fallback, notes and local-history browsers, Line Inspector) and fails on any `.monospace` or `GtkTextView:monospace` widget not registered
- [ ] 4.5 The existing dense restyle widget tests (paced reveal, a second change during a reveal, several dense tabs) still pass unchanged
- [ ] 4.6 Widget test: two windows, a zoom step in one swaps the classes in both and loads no provider; a font change loads one provider in a dispatch that swaps no class, then swaps in a later one; a zoom level of 55 set through GSettings loads once and two steps after it load nothing

## 5. Lane and verdict

- [ ] 5.1 Lane: the populated restyle case (sidebar shown over 300 entries), its control (same geometry, sidebar shown over an empty folder) and its search-results variant (D7), with the per-phase difference to the control in the report; self-test
- [ ] 5.2 Interleaved verdict, `main` against the branch, host load below 4 recorded before and after, `btrfs-stall-report` for the window: the dense restyle cases and the populated case; apply the adoption criterion (design D5) and record every residual over `B_INTERACT_US` against C6's text re-layout floor, the search-results variant's label re-layout, and each font-change dispatch (the load and the swap) against `B_SETTLE_US`, with the load's whole-tree revalidation as the A86 exception where it exceeds it
- [ ] 5.3 `make check`, `make test` (0 FLAKY), `make gtk-axioms`, `make visual-geometry-smoke`, `make visual-smoke`, `make accessibility-smoke`

## 6. Documentation

- [ ] 6.1 Programme record (`docs/next/long-line-documents.md`): CF row and an implementation record, the file-tree correction of C6's note (and of the CF row's "`.monospace` file tree"), footnote ³ of C3 (the font change's owner and its new figures), the verdict tables
- [ ] 6.2 `.agents/rules/ui.md` (GSettings bindings, font customization), AGENTS.md (key design decision, module layout for `ui/font_zoom.rs`, the stale "file tree labels use `.monospace`" sentence), the stale `ui/window/zoom.rs` module comment ("and the sidebar file tree"), the `ui/theme.rs` provider comments, README if the zoom behaviour description changes, the gtk-axiom ledger
- [ ] 6.3 `docs/workflow-readability-matrix.md` if the new module needs a cross-cutting row; `make check-workflow-boundaries`
