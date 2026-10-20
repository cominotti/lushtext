## Why

LushText must be instantly responsive for every file it opens, and opening starts with
constructing the window. The long-line investigation (`docs/next/long-line-responsiveness.md`,
"Why" item 8) measured a **188–380 ms main-loop stall at window construction for every file**,
including a small one: `load/runs/*/result.json` reports the first ping blocked for 188.7 ms
(`A-nosample`) to 369.8 ms (`A-default`), and 332.6 ms on `smoke-small`. That is 12 to 24 times the
stream's working settle budget (B_settle = 16 ms per settle dispatch, a maintainer decision of
2026-10-11 that C0 confirms by measurement).

The one stack sampled inside window construction (`load/runs/A-default-2/stacks.json`, t = 0.205 s)
is a synchronous `fsync` on the GTK thread:
`LushtextWindow::constructed` → `geometry::execution::migrate_split_view_settings` →
`Settings::set_double` → `g_keyfile_settings_backend_write` → `g_file_replace_contents` → `fsync`.
`migrate_split_view_settings` makes five GSettings writes on a fresh profile. Three more writes happen
unconditionally on **every** construction, steady-state profiles included: `restore_workspace_split_view`
and `restore_properties_split_view` each make one, and `preview::setup_preview_actions`, called from
`LushtextWindow::new`, resets `preview-pane-visible` to `false` (`ui/window/preview.rs:45`). The doc
comment on `LushtextWindow::new` ("this constructor performs no disk writes") is therefore false. Under
the GSettings keyfile backend, every write rewrites the settings file and fsyncs it on the calling
thread; GSettings does not skip a write whose value is unchanged (expected from the GIO sources; axiom
G1 confirms it). The probe lane used the keyfile backend. The shipped Flatpak grants no dconf access, so it is
expected to use the same backend; task 1.2 verifies this. The same GTK-thread fsync also follows every
`default-width`/`default-height` notification while the user resizes the window, and every sidebar,
properties, preview, search-panel, colour-scheme, invisible-characters, show-hidden-files and minimap
toggle, the workspace sidebar preset change, and the Preferences dialog's imperative writes (style
scheme, custom font, sidebar preset, excluded names). Those writes break B_interact (8 ms), which makes
them pre-existing blockers in this stream.

The second sample (`load/runs/A-default/stacks.json`, t = 0.207 s) shows `gtk_init` making a
synchronous portal check over D-Bus. That cost belongs to the toolkit's startup. Two coarse samples
cannot apportion the stall, so this change starts with a failing-first, span-level measurement.

The change does not depend on the long-line presentation work (C1–C3), but it belongs to the same work
stream and the same responsiveness invariant, and it depends on C0 (`probe-long-line-toolkit-costs`)
for the budget constants, the watchdog, frame timing and the measured smoke lane. Stream order is
C0 → C1 → C2 → (C3) → C4 → (C5), so C0 is applied before this change is implemented.

## What Changes

- **Failing-first measurement of the construction path.** Record monotonic spans for LushText's
  startup handler, window construction (`LushtextWindow::new` until control returns to the main loop),
  and each GTK dispatch up to the first presented frame. Measure on the release build, on a fresh
  profile and on a steady-state profile, under both the keyfile and the dconf GSettings backends. Use
  C0's watchdog and frame timing (C0 is applied first). The spans are captured before any fix and
  recorded as the failing baseline.
- **No GSettings backend write during window construction.** The preview-pane reset goes through the
  change-only writer, and the doc comment on `LushtextWindow::new` is made true. The split-view
  migration and both restore steps become a pure plan in `ui/window/geometry/policy.rs` (whole-pixel, Kani-checked). The
  plan's values are applied to the widgets immediately. Their persistence is change-only and deferred.
  A steady-state launch writes nothing. A first launch writes once, after the first presented frame.
- **One coalescing GSettings writer for imperative LushText writes** (cross-cutting coordination, no
  role). It is change-only and last-writer-wins, and it flushes at most once per persistence window.
  Every imperative LushText write goes through it: window-size and maximized notifications, the
  sidebar, properties, preview and search-panel toggles, the colour-scheme, invisible-characters and
  boolean setting-toggle actions, the workspace preset and excluded-names writes, the Preferences
  dialog's imperative writes, and the geometry restore, migration and preview-reset writes. Until a
  value is flushed, every LushText reader of a routed key reads it through the writer's overlay, and
  every LushText `changed` subscriber of a routed key is notified by the writer at enqueue time, so
  read-your-writes and the immediacy of cross-instance effects (editor pages reacting to invisible
  characters, the window reacting to the Preferences preset) are preserved. Where its backend write
  executes (on a worker thread or on a deferred GTK idle) is decided by a new axiom probe of the
  keyfile backend's locking and fsync behaviour. Whichever path is chosen must keep every GTK dispatch
  within budget. The last window's `close_request` and application shutdown flush synchronously, so
  the persisted window state is exact; closing a non-last window hands its final values to the writer
  and does not stall the remaining windows.
- **Staged construction, if the measurement still flags residual work.** If the residual LushText-owned
  construction span still exceeds B_settle once settings writes are gone, the surfaces the first frame
  does not need are built in later bounded settle dispatches, each within B_settle. An action that
  needs such a surface before its stage runs forces that stage synchronously, and that stage is bounded
  as well.
- **Deterministic PR gate.** A widget test counts GSettings backend writes during construction: zero on
  a steady-state profile and zero before the first frame on a fresh profile. It also proves the
  migrated values match the previous behaviour.
- **Measured lane.** Construction and first-frame spans join the responsiveness or performance smoke
  lane. The report includes the profile state, the backend, the build profile, and
  `btrfs-stall-report` evidence for the run window.
- **Evidence and automation.** A bounded `window_construction` evidence projection (span durations in
  microseconds, the stage count, and the number of settings writes made before the first frame) is
  added to the shell-geometry evidence and to the automation snapshot. It carries no content.
- **GTK axiom ledger growth.** Ledger entries, each with a `gtk-lush-axioms` probe and sample, for the
  GSettings backend behaviours the design depends on: synchronous keyfile write and fsync on the
  calling thread, delay/apply coalescing, and cross-thread write and read locking.

## Capabilities

### New Capabilities

None. The behaviour extends existing capabilities.

### Modified Capabilities

- `main-thread-responsiveness`: add requirements that window construction and the dispatches up to the
  first frame fit B_settle, and that GSettings backend writes never execute synchronously on the GTK
  thread during construction or on interaction paths.
- `adaptive-editor-geometry`: add a requirement that the split-view settings migration and restore are
  a pure, Kani-checked plan, applied to the widgets without a construction-time settings write.
- `performance-regression-coverage`: add a requirement covering the deterministic construction
  write-count gate and the measured construction and first-frame span lane.
- `dbus-automation-spine`: add the bounded `window_construction` snapshot projection.
- `gtk-axiom-ledger`: add a requirement that the GSettings backend behaviours the shell depends on are
  ledger axioms with probes.

## Impact

- **Code:**
  - `crates/lushtext-core/src/ui/window/geometry/{policy,execution,evidence,mod,kani_proofs}.rs` and
    `policy/kani_proofs.rs`.
  - `ui/window/imp.rs` (construction order and the size/maximized notify handlers).
  - `ui/window/{actions,preview,search,mod}.rs`, `ui/window/encoding/execution.rs`,
    `ui/workspace_visibility.rs`, `ui/preferences/{imp,excluded_names}.rs` and
    `app.rs`'s `register_boolean_setting_toggle_action` (imperative writes routed to the writer), plus
    the readers and `changed` subscribers of routed keys (`ui/window/imp.rs`'s workspace-fraction
    subscription, `geometry/{execution,evidence}.rs`, `ui/editor_page/imp.rs`'s invisible-characters
    read, `ui/preferences/imp.rs`).
  - A new cross-cutting `ui/settings_writes.rs`, with its pure half in `model/settings_write_batch.rs`
    (unit and property tests, plus a Kani harness).
  - The `app.rs` startup span.
  - Conditionally, a staged-construction module under `ui/window/` and template changes in
    `resources/ui/*.blp`, with the generated `.ui` files.
  - `ui/automation.rs` for the snapshot projection.
- **GTK Lush:** new `gtk-lush-axioms` probes and samples. `make gtk-axioms` runs first.
- **Tests:** a new widget test module, assigned in `scripts/widget-shards.py`. Kani harnesses go in an
  existing measured shard, or in a new one in `scripts/kani-shards.py`. Smoke-lane scenarios, synthetic
  only.
- **Docs:**
  - `AGENTS.md` (module layout and the window-state persistence decision);
  - `.agents/rules/build.md` and `.agents/rules/ui.md` (GSettings write rule);
  - `docs/workflow-readability-matrix.md` (re-derived `WFR-SHELL-GEOMETRY` cells and those of every
    other live row whose files the change edits; `WFR-SHELL-LAYOUT` is `superseded` and is not edited;
    the cross-cutting writer declared);
  - `docs/next/formal-verification.md` (the new axioms and Kani harnesses);
  - `docs/automation.md` and `docs/automation-reference.md`;
  - the GTK axiom ledger;
  - the stream's `docs/next/` record;
  - `docs/end-user-coverage.md`.
- **Packaging:** none, unless the maintainer chooses dconf access for the Flatpak (an open question).
  That choice would change the Flatpak permission posture and its docs.
- **User data:** no file content is touched. Persisted GSettings values and their migration results are
  unchanged. Only when and on which thread they are written changes.
