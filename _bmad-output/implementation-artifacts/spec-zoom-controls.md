---
title: 'Zoom Controls'
type: 'feature'
created: '2026-04-04'
status: 'done'
baseline_commit: 'd020c598'
context: ['.claude/CLAUDE.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** LushText has no zoom controls. Users cannot adjust editor font size without changing the font in Preferences. GNOME Text Editor provides zoom in/out/reset via a hamburger menu widget and keyboard shortcuts.

**Approach:** Add a zoom widget ([−] [100%] [+]) to the hamburger menu matching GNOME Text Editor's layout, with keyboard shortcuts (Ctrl+=/Ctrl+-/Ctrl+0), GSettings persistence, and command palette entries. Zoom modifies the display-wide `.monospace` CSS font-size relative to the base font.

## Boundaries & Constraints

**Always:**
- Match GTE's zoom widget visual layout: circular flat zoom-out, pill-style percentage label (doubles as reset button), circular flat zoom-in
- Zoom affects all `.monospace` widgets (editor + sidebar file tree) via the existing display-wide CSS provider
- Disable zoom-out button at 50%, zoom-in button at 400%
- Persist zoom level across sessions via GSettings

**Ask First:**
- If `window/mod.rs` exceeds 1000 lines after adding zoom setup, confirm extraction strategy

**Never:**
- Per-page zoom (all editors share one zoom level)
- Touch the existing theme selector or fullscreen wiring
- Change font Preferences behavior (zoom is independent of custom font choice)

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Zoom in | Ctrl+= at zoom < 400 | Zoom +10%, label updates, CSS re-applied | N/A |
| Zoom in at max | Ctrl+= at 400% | No change, zoom-in button disabled | N/A |
| Zoom out | Ctrl+- at zoom > 50 | Zoom -10%, label updates, CSS re-applied | N/A |
| Zoom out at min | Ctrl+- at 50% | No change, zoom-out button disabled | N/A |
| Reset zoom | Ctrl+0 or click percentage label | Zoom set to 100%, label shows "100%" | N/A |
| Base font change while zoomed | Change custom-font or toggle use-system-font at zoom ≠ 100% | CSS recalculated with new base + current zoom | N/A |
| App restart | zoom-level=130 in GSettings | Zoom restored at 130%, correct font size | N/A |

</frozen-after-approval>

## Code Map

- `data/dev.cominotti.lushtext.gschema.xml` -- add `zoom-level` key (uint, default 100, range 50–400)
- `crates/lushtext-core/src/config.rs` -- add `ZOOM_LEVEL` constant
- `crates/lushtext-core/src/lib.rs` -- modify `apply_font_css()` to incorporate zoom-level; connect zoom-level setting changes
- `resources/ui/window.ui` -- add `<attribute name="custom">zoom</attribute>` slot in hamburger menu
- `resources/style/style.css` -- add zoom widget CSS (margins, spacing to match theme selector)
- `crates/lushtext-core/src/ui/window/mod.rs` -- add module declaration, setup calls, zoom keyboard shortcuts
- `crates/lushtext-core/src/ui/window/zoom.rs` -- NEW: zoom widget + actions (extracted to keep mod.rs near 1000 lines)
- `crates/lushtext-core/src/services/palette.rs` -- add Zoom In, Zoom Out, Reset Zoom command defs

## Tasks & Acceptance

**Execution:**
- [x] `data/dev.cominotti.lushtext.gschema.xml` -- add `zoom-level` key (uint, default 100, range 50–400)
- [x] `crates/lushtext-core/src/config.rs` -- add `ZOOM_LEVEL` constant string
- [x] `crates/lushtext-core/src/lib.rs` -- modify `apply_font_css()` to read zoom-level and compute `base_size * zoom_level / 100`; connect zoom-level changes; when `use-system-font=true` and zoom ≠ 100%, resolve system monospace size via `pango::FontDescription`
- [x] `resources/ui/window.ui` -- add zoom custom widget slot in hamburger menu (same section as theme selector, after it)
- [x] `resources/style/style.css` -- add `.zoom-controls` styles (margins/spacing matching theme selector row)
- [x] `crates/lushtext-core/src/ui/window/mod.rs` -- add `setup_zoom_controls()` to build [−][100%][+] widget and insert via `PopoverMenu::add_child("zoom")`; add `zoom-in`/`zoom-out`/`zoom-reset` actions with enabled-state management; register keyboard accels
- [x] `crates/lushtext-core/src/services/palette.rs` -- add Zoom In (Ctrl+=), Zoom Out (Ctrl+-), Reset Zoom (Ctrl+0) to `COMMANDS` in View category

**Acceptance Criteria:**
- Given the app is running, when user presses Ctrl+=, then zoom increases by 10% and hamburger label updates
- Given zoom is at 400%, when user presses Ctrl+= or clicks zoom-in, then nothing happens and button is disabled
- Given zoom is at 50%, when user presses Ctrl+- or clicks zoom-out, then nothing happens and button is disabled
- Given zoom ≠ 100%, when user presses Ctrl+0 or clicks percentage label, then zoom resets to 100%
- Given zoom is 130%, when user restarts the app, then zoom is restored at 130% with correct font size
- Given zoom is 120% and user changes custom font, then new base size is scaled by 120%
- Given command palette is open, when user types "zoom", then Zoom In/Out/Reset commands appear

## Design Notes

**Percentage-based zoom (diverges from GTE):** GTE uses integer pt-delta steps (+1pt/-1pt). LushText uses percentage steps (+10%) stored as the zoom value directly (100 = 100%). Clean bounds (50–400), self-documenting GSettings value, label = value + `%` suffix.

**System font base size resolution:** When `use-system-font=true` and zoom ≠ 100%, `apply_font_css()` resolves the system monospace size via `pango::FontDescription::from_string("Monospace")` → `size() / pango::SCALE` to compute the absolute pt value for CSS.

## Verification

**Commands:**
- `make check` -- expected: no clippy warnings or fmt errors
- `make test` -- expected: all existing tests pass

**Manual checks:**
- `make run` -- zoom widget visible in hamburger menu; Ctrl+=/Ctrl+-/Ctrl+0 work; label updates correctly; zoom persists on restart; no GTK/pixman warnings on stderr

## Suggested Review Order

**Zoom widget and actions**

- Entry point: widget construction, GSettings wiring, popover insertion
  [`zoom.rs:21`](../../crates/lushtext-core/src/ui/window/zoom.rs#L21)

- Window actions: zoom-in/out/reset with bounds clamping
  [`zoom.rs:123`](../../crates/lushtext-core/src/ui/window/zoom.rs#L123)

**Font CSS scaling**

- Core zoom logic: resolves base font, applies zoom percentage, safe schema lookup
  [`lib.rs:112`](../../crates/lushtext-core/src/lib.rs#L112)

- CSS re-apply trigger: zoom-level added to connect_changed loop
  [`lib.rs:101`](../../crates/lushtext-core/src/lib.rs#L101)

**Integration wiring**

- Module declaration and setup calls in window constructor
  [`mod.rs:12`](../../crates/lushtext-core/src/ui/window/mod.rs#L12)

- Keyboard shortcuts: Ctrl+=/Ctrl+-/Ctrl+0 with keypad variants
  [`mod.rs:840`](../../crates/lushtext-core/src/ui/window/mod.rs#L840)

- Command palette entries: Zoom In/Out/Reset in View category
  [`palette.rs:293`](../../crates/lushtext-core/src/services/palette.rs#L293)

**Schema and config**

- GSettings key: zoom-level uint 50-400, default 100
  [`gschema.xml:53`](../../data/dev.cominotti.lushtext.gschema.xml#L53)

- Config constant
  [`config.rs:24`](../../crates/lushtext-core/src/config.rs#L24)

**Styling**

- Menu XML: zoom custom widget slot after theme selector
  [`window.ui:141`](../../resources/ui/window.ui#L141)

- CSS: zoom-controls margins matching theme selector
  [`style.css:113`](../../resources/style/style.css#L113)
