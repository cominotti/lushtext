# Quickstart: File Peek

## Goal

Implement a sidebar-only, read-only file peek that lets users inspect files with
`Space` without creating tabs, while promotion still uses the existing
open-document workflow.

## Recommended implementation order

1. Add a GTK-free `services/file_peek.rs` module and wire it into
   `crates/lushtext-core/src/services/mod.rs`.
2. Add a `peek.rs` workflow module under
   `crates/lushtext-core/src/ui/sidebar/workspace_section/`.
3. Extend `workspace_section/imp.rs` with the reusable popover, request state,
   and any preview widgets or labels needed by the card.
4. Hook `Space`, `Escape`, selection-change refresh, and invalidation paths from
   the section's existing `GtkListView` and `SingleSelection`.
5. Reuse the existing file-activation callback surface for promotion so the
   window still owns `open_document()` and duplicate-tab reuse.
6. Update `docs/next/file-peek.md`, `README.md`, and `AGENTS.md` only if the
   shipped module structure or UX contract changed during implementation.

## Service-level verification

Run these after the bounded snapshot loader exists:

```bash
make test-unit
```

Focus the new unit coverage on:

- preview eligibility and fallback classification
- bounded text truncation metadata
- invalid UTF-8 or unreadable file handling
- stale generation or request suppression helpers

## Widget-level verification

Run the GTK harness with the same headless path used in CI:

```bash
make test-widget-headless
```

Expected widget coverage:

- `Space` opens peek for the selected file row without creating a tab
- Up and Down keep selection in the sidebar and refresh peek in place
- `Escape`, repeated `Space`, and click-away dismiss correctly
- promotion reuses an existing tab instead of creating a duplicate
- dismissal returns focus to the sidebar selection when promotion did not occur
- section rebuild, workspace filter hide, or row invalidation close the peek cleanly

## Runtime validation

Run a live session and verify the real interaction contract:

```bash
make run
```

Manual checks:

1. Test `Space`, `Escape`, Up and Down, and `Enter` on regular text files.
2. Repeat the same flow at Small, Comfy, and Large sidebar presets.
3. Verify the card overlaps the center area instead of resizing split panes.
4. Try directory rows, placeholders, binary files, unreadable files, and a file
   above the open refusal threshold.
5. Confirm dismissal returns navigation to the sidebar and promotion returns
   focus to the editor.
6. Watch stderr and confirm there are no GTK, GLib, or pixman warnings.

## Latency spot-check for SC-002

Use the same live session to time the user-visible response for eligible local
text files:

1. Prepare 20 eligible local text files that are representative of normal peek use.
2. Trigger peek with `Space` for each file while keeping sidebar navigation active.
3. Measure from the `Space` keypress to visible preview content or an explicit
   fallback state with a screen recording, stopwatch, or equivalent timing aid.
4. Pass the spot-check when at least 19 of 20 attempts complete within `0.25`
   seconds and none freeze keyboard navigation or leave the UI stranded.

## Full acceptance gate

Run the repo checks before sign-off:

```bash
cargo bench -p lushtext-core --no-run
make check
```
