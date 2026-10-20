## Context

**The stream.** C4 of the long-line stream (`docs/next/long-line-responsiveness.md`). It does not depend
on long lines, but it serves the same invariant. The maintainer set that invariant on 2026-10-11:

- every settle dispatch (open, install, restore, eviction reload) fits **B_settle = 16 ms**;
- every interaction dispatch fits **B_interact = 8 ms**;
- input-to-presented-frame takes at most 2 frames.

These are working values. C0 (`probe-long-line-toolkit-costs`) confirms them by measurement and records
them as policy constants. C4 consumes those constants and does not define its own.

**Evidence, read honestly.**

- `load/run.sh` creates a fresh `XDG_CONFIG_HOME` for every run and sets `GSETTINGS_BACKEND=keyfile`.
  So every measured run was a **first launch on the keyfile backend**. The stall reproduces "for every
  file" because every run was a first launch, not because of anything in the file.
- `probe.py` measures the main-loop latency of a D-Bus property read. The first ping waits until
  `g_application_run` returns to the loop, so the 188–380 ms covers three things together: GTK startup
  (`gtk_init`, including its synchronous portal check), LushText's `startup`, and `open` →
  `LushtextWindow::new`.
- Only two eu-stack samples, taken 200 ms apart, fall inside that window:
  - one in `gtk_init`'s `check_portal_interface`;
  - one in an `fsync` reached from `migrate_split_view_settings` → `set_double`.
- The binary was `target/debug`. Debug timings overstate Rust-side costs.

**Code.** `LushtextWindow::constructed` (`ui/window/imp.rs:690-725`) does the following in order:

1. reads window size;
2. `configure_split_views`;
3. `migrate_split_view_settings` (`geometry/execution.rs:112-134`): five writes on first launch, an
   early return afterwards;
4. `install_split_view_breakpoints`;
5. `restore_workspace_split_view`: one unconditional `set_double`;
6. `restore_properties_split_view`: one unconditional `set_double`, whose value depends on the width.

It then connects `default-width`, `default-height` and `maximized` notify handlers, each of which writes
synchronously. After `constructed`, `LushtextWindow::new` (`ui/window/mod.rs:170-191`) calls
`preview::setup_preview_actions`, which writes `preview-pane-visible = false` unconditionally
(`preview.rs:45`). So a **steady-state** construction makes three synchronous writes (the two restores
and the preview reset), and a fresh one makes eight. The doc comment on `LushtextWindow::new` claims
"this constructor performs no disk writes"; it is false today and must be true after this change.

Imperative writes on interaction paths (the audit in task 3.1 starts from this list and must find any
it misses):

- `actions.rs:485, 577` (sidebar and properties visibility), `preview.rs:67`, `search.rs:603, 619`,
  `mod.rs:264-284` (colour scheme), `geometry/execution.rs:396` (workspace preset);
- `encoding/execution.rs:318, 351` (`invisible-characters-mode`);
- `app.rs`'s `register_boolean_setting_toggle_action` (`app.show-hidden-files`, `win.toggle-minimap`);
- `workspace_visibility.rs:38` (`workspace-excluded-names`);
- `preferences/imp.rs:447, 488, 504` (sidebar preset, style scheme, custom font) and
  `preferences/excluded_names.rs:61` (`reset`).

Preferences rows also use two-way `Settings::bind`.

Several routed keys have **readers and `changed` subscribers on other `gio::Settings` instances or in
later dispatches**, so "the writer never re-reads its own key in the same dispatch" is not enough:

- `ui/window/imp.rs:810` subscribes to `workspace-sidebar-width-fraction` (the Preferences preset);
- `geometry/execution.rs:284` re-reads that key on every allocation-time reconciliation;
- `geometry/evidence.rs:136-137` reports both persisted fractions;
- editor pages read `invisible-characters-mode` (`editor_page/imp.rs:728`) and react to its `changed`;
- the boolean toggle actions and the sidebar react to `changed` on their keys.

**Backends.**

- Under `GSETTINGS_BACKEND=keyfile` (all smoke lanes; the probe), a write is `g_file_replace_contents`
  plus `fsync` on the calling thread. The sampled stack confirms this.
- The shipped Flatpak's finish-args grant no dconf access, so GLib inside the sandbox is expected to use
  the keyfile backend. This is an **assumption** that task 1.2 verifies.
- The Snap uses the `gsettings` interface, which is dconf.
- dconf writes are applied in-process and sent asynchronously, but each write still costs a D-Bus send.
- On this workstation (btrfs RAID1, dm-crypt, `flushoncommit`), one fsync can take tens to hundreds of
  milliseconds.

**Roles.** `ui/window/geometry/` is the migrated role home of `WFR-SHELL-GEOMETRY`:

- `policy.rs` is whole-pixel and `forbid`s the float lint pair;
- `execution.rs` holds coordination, `evidence.rs` holds the evidence surface, and `kani_proofs.rs` and
  `policy/kani_proofs.rs` hold the proofs.

`WFR-SHELL-LAYOUT` is `superseded` (slot 7b) and is not edited. The construction order and the size and
maximized handlers in `ui/window/imp.rs` are re-derived under whichever live row the matrix assigns
those lines to (`WFR-SHELL-GEOMETRY` for the geometry calls); task 7.4 reads the assignment from the
matrix rather than assuming it.

## Goals / Non-Goals

**Goals:**

- A failing-first, span-level baseline of startup, window construction and every dispatch up to the
  first presented frame. It covers the release build, fresh and steady-state profiles, and the keyfile
  and dconf backends.
- Window construction, and each GTK dispatch until the first presented frame, fit B_settle on the
  reference lane (private `mutter --headless`, `GSK_RENDERER=cairo`, ≤16 cores, release build).
- Zero GSettings backend writes before the first presented frame. On a steady-state launch, zero writes
  at all.
- No synchronous GSettings backend write on the GTK thread on an interaction path (resize, toggles,
  colour scheme), unless the probe shows that one write fits B_interact on the keyfile backend
  (decision D3).
- Persisted values are unchanged. The migration result is identical, and window size and maximized
  state are exact after close.
- A deterministic PR gate and a measured lane.

**Non-Goals:**

- GTK's own `gtk_init` cost, including the synchronous portal check. It is measured and reported as the
  toolkit startup span, but it is not LushText-owned and not budgeted here. An upstream report is listed
  as an open question.
- Long-line, Protected View, syntax or minimap work (C1–C3).
- Changing any schema key, default, or persisted format.
- Replacing GSettings.

## Decisions

### D1. Measure first, with spans rather than pings

A GTK-free span recorder (`Instant` pairs held in plain data) records these spans:

- `startup_lushtext`: `LushtextApplication::startup` after `parent_startup`;
- `startup_toolkit`: `parent_startup`, mostly `gtk_init`;
- `window_construction`: `LushtextWindow::new`, until control returns to the main loop;
- each main-loop dispatch until the first `after-paint` on the window's frame clock.

Dispatch lengths come from C0's `G_PRIORITY_HIGH` lateness watchdog. C0 is applied before this change
(task 1.1, stream order C0 → C1 → C2 → (C3) → C4 → (C5)), so no temporary equivalent is built.

The baseline runs before any fix, on the release build, with each combination of profile (fresh or
steady) and backend (keyfile or dconf), and is recorded in the stream's `docs/next/` record. It must
reproduce a construction span above B_settle. A steady-state profile still makes three synchronous
writes today, so it is expected to reproduce as well; if it does not, the record says
so explicitly and the fresh-profile case carries the failing-first evidence.

**Alternative rejected:** reusing the D-Bus ping. It measures queueing and cannot separate toolkit
startup from window construction.

### D2. The split-view migration and restore become a pure plan

`policy.rs` gains `plan_split_view_restore(facts) -> SplitViewRestorePlan`.

- **Facts** (all float-free; `policy.rs` `forbid`s `clippy::float_arithmetic` and the disallowed float
  methods, so every comparison against a stored `f64` happens in `execution.rs`, which reduces it to a
  boolean fact):
  - the `migrated` flag;
  - legacy `sidebar-visible` (stored or default);
  - the restored window width (whole sp) used by the migration, and the current window width (whole
    sp, `current_window_width`) used by the restores, which today may differ;
  - the stored workspace preset (snapped by `WorkspaceSidebarWidthPreset::from_fraction`), and
    `stored_workspace_is_exact_preset`: whether the stored fraction already equals that preset's
    fraction;
  - `stored_properties_matches_desired`: whether the stored properties fraction already equals
    `split_fraction(desired_properties_share(current width))`;
  - both visibility keys.
- **The plan contains:**
  - the effective visibility intents;
  - the workspace preset;
  - the workspace and properties `PaneShare`s;
  - a `PersistIntent`: the set of keys whose persisted value differs from what the plan implies.

It stays whole-pixel: shares and presets, never fractions. `execution::split_fraction` forms the
fractions once, as it does today. `execution.rs` applies the plan to the widgets in the same order as
now, so the same breakpoint is installed and the same widths are restored. It then hands
`PersistIntent` to the settings writer (D3) instead of calling `set_*`. A Kani harness in
`policy/kani_proofs.rs` proves the following, for every `i32` width and every flag combination:

- (a) a migrated profile with `stored_workspace_is_exact_preset` and
  `stored_properties_matches_desired` yields an empty `PersistIntent`;
- (b) a fresh profile yields exactly the five migration keys, and each of the two width-fraction keys
  carries the value today's code leaves persisted last: the restore's value, which overwrites the
  migration's (the restore keys are a subset of the migration keys, so last-writer-wins, not a union
  of seven writes);
- (c) the effective intents equal what the current code computes (oracle: a transcription of today's
  `migrate_split_view_settings` and `restore_*` into pure form, kept as the `#[cfg(kani)]` and unit-test
  reference);
- (d) properties visibility after migration is `false`.

**Amended by the simplify pass.** The plan no longer carries a `PersistIntent`, and the facts no
longer carry the restored width or the two stored-fraction booleans. The settings writer already
drops a value equal to the current or persisted one (D3), so the adapter hands it the values the
pre-plan code wrote (the migration keys on a fresh profile, both fractions always, with the values
left persisted last) and the writer decides what reaches the backend with exact equality. (a) and
(b) became the writer's own properties; the harness proves (c) and (d), and the default preset on a
fresh profile. The migration's restored-width fraction was always overwritten by the restore's, so
that fact never reached the store.

**Alternative rejected:** keeping the writes and only deferring them to an idle. A synchronous fsync in
a later idle still breaks B_settle.

### D3. One coalescing settings writer; its execution path is probe-decided

The writer is cross-cutting coordination with no role, in `ui/settings_writes.rs`.

- **One per application.** It owns a pending map from key to `glib::Variant`. A write takes the value
  in memory at once and enqueues it. A value equal to the last value known to be persisted is dropped.
  A flush is scheduled after the first presented frame at the earliest, and is then coalesced to at
  most one per persistence window (a `gtk_lush_settle::SupersedingTimer`, which removes a superseded timeout when re-armed; working value 500 ms).
- **Pure half.** The pending-set algebra is GTK-free in `model/settings_write_batch.rs`:
  last-writer-wins, change-only, and flush snapshot or acknowledge, generic over keys and values. It
  has unit and property tests, and a Kani harness over bounded sequences: a flush never drops the
  latest value of any key, an acknowledged flush never re-emits an equal value, and an empty batch
  flushes nothing.
- **Read-your-writes.** The GSettings value of a routed key is stale from enqueue until the flush,
  which is at least one persistence window later, and several readers read it in later dispatches
  (see Code above). So every LushText read of a routed key goes through the writer's overlay
  (`SettingsWrites::value(key)`: the pending value if any, else the `gio::Settings` value). Task 3.1
  enumerates every reader, and a policy check (task 3.7) forbids a direct `gio::Settings` read of a
  routed key outside the writer.
- **Cross-instance `changed`.** Other `gio::Settings` instances see a change only when the flush
  reaches the backend. Several routed keys drive their effect through `changed` on another instance
  (editor pages and `invisible-characters-mode`; the window and the Preferences preset; the boolean
  toggle actions; the sidebar and the visibility keys). So the writer notifies LushText subscribers of
  a routed key at enqueue time, through its own `connect_changed(key)`; those subscribers stop
  subscribing to `gio::Settings::changed` for that key directly and receive external changes (another
  process, `gsettings set`) from the writer, which forwards the backend's `changed` and suppresses the
  echo of its own flush (the flushed value equals the overlay value, so the forward is change-only).
  The colour-scheme actions also apply `StyleManager` directly.
- **Seeding change-only.** The last-persisted value of a key is seeded by reading the backend on its
  first enqueue (a keyfile or dconf read is in memory, no I/O).
- **Where the flush runs is decided by the new axiom probes (D5):**
  1. **Worker flush (preferred).** A dedicated `gio::Settings` instance is created and used only on one
     long-lived writer thread. It is used only if the probe shows that a keyfile write and fsync on
     that thread does not block GTK-thread reads or writes of another instance for the fsync's
     duration.
  2. **Fallback.** If it does block, the flush runs on a GTK idle at low priority after the first
     frame, as one `delay()`/`apply()` batch: one backend write per window. The measured lane must then
     show that the flush dispatch fits B_settle on the reference lane. If it does not, the
     maintainer's dconf-access decision (Open Question 1) becomes a blocker for this change, recorded
     in the stream record. It is not accepted as debt.
- **Close and shutdown.** The writer is per application, so a window's close does not need a
  synchronous flush while other windows remain: a non-last window's `close_request` enqueues its final
  size and maximized state and returns, so the remaining windows never wait for an fsync. The last
  window's `close_request` (part of the confirmed-close transaction, which already saves the session
  synchronously and leaves no other window to keep responsive) and `ApplicationImpl::shutdown` (a
  backstop) flush synchronously. A synchronous flush first waits for any in-flight worker flush to
  finish, so the two can never reorder and the newest value is the one left on disk.
- **No frame.** If a window is never presented, nothing is flushed before close; the close or shutdown
  flush persists it. A crash first loses only the pending layout values, and the migration simply
  re-runs on the next launch (it is idempotent).
- **Preferences binds** (`Settings::bind`) write through GIO directly. Task 3.5 measures one toggle on
  the keyfile backend. If it exceeds B_interact, those rows bind against the writer's delayed instance,
  and the writer flushes them through the same path. Otherwise the measurement is recorded and the
  binds stay as they are.

**Alternatives rejected:**

- A LushText-owned keyfile: it duplicates GSettings and breaks the schema.
- Calling the backend from a raw thread without a probe: it might move the stall into a lock wait on
  the GTK thread.

### D4. Staged construction is conditional on the residual measurement

After D2 and D3, the baseline is re-run. If `window_construction` or any dispatch before the first frame
still exceeds B_settle on the release build, the recorder's per-stage breakdown names the dominant
costs. Candidates are template instantiation of secondary surfaces (search panel, properties panel,
Markdown preview, notes chrome) and eager signal wiring.

Surfaces that the first frame does not show are moved behind a `ConstructionStage` sequence. Each stage
is built in its own idle dispatch within B_settle, in a fixed order. Each stage has a
`*_ensure_constructed` entry that an action reaching it early calls synchronously, and that entry is
bounded by the same budget.

Template children that move become lazily built children of an `AdwBin` placeholder. The
`resources/ui/*.blp` sources and the generated `.ui` files are changed together (`make check-blueprint`).
Readiness: `WaitForReady` predicates that depend on a staged surface also wait for its stage. A stage
that has not yet been built counts as a readiness blocker, documented in the automation docs.

Accessibility: a staged surface applies its accessible names, roles and relations when its stage is
built, exactly as the eager template does today. If a stable AT-SPI anchor in
`docs/accessibility-matrix.md` or a step in `make accessibility-smoke` would now depend on a stage, the
smoke waits for the stage's readiness and `docs/accessibility.md` and the matrix are updated. Keyboard
paths that reach a staged surface go through its `*_ensure_constructed` entry.

If the measurement shows no residual, D4 is recorded as not needed and its tasks are closed with that
evidence.

### D5. Axioms for the GSettings backend

GIO is part of the toolkit stack the shell depends on. The ledger gains entries, with the next free ids
at apply time (C0 reserves A23–A36). Each entry has a `gtk-lush-axioms` probe and a sample with an
interactive mode and a `--check` mode:

- (G1) a keyfile-backend write rewrites and fsyncs the settings file synchronously on the calling
  thread, once per `set_*`;
- (G2) `delay()` plus `apply()` performs one backend write for N pending keys;
- (G3) whether a write on one thread's instance blocks reads and writes on another thread's instance,
  sharing the default backend, for the duration of the fsync;
- (G4) whether `changed` on a second instance is emitted only after the backend write.

`make gtk-axioms` runs first. Probe outcomes feed D3's choice. Each probe runs against a private keyfile
directory under its own temporary scope.

### D6. Evidence and automation

The shell-geometry evidence gains `WindowConstructionEvidence`:

- `startup_lushtext_us`, `startup_toolkit_us`, `window_construction_us`, `first_frame_us`;
- the largest dispatch before the first frame, which needs C0's watchdog; C0 feature-gates that
  watchdog, so in a build without the feature the field is `null`, never `0`;
- `stages_pending` and `stages_total`;
- `settings_writes_before_first_frame`;
- the writer's pending, flushed and dropped-equal counters.

The automation snapshot projects it as a bounded `window_construction` object: integers only, no paths,
no content. The automation docs, the reference and `make check-automation-docs` are updated.

### D7. Gates

- **Deterministic PR gate (widget).** A new module, assigned in `scripts/widget-shards.py`. It
  constructs a window under the private keyfile backend with a steady-state profile and asserts zero
  backend writes before and after the first frame (today this profile makes three). It first confirms
  that the harness delivers the window frame clock's `after-paint` (presented widget tests do not
  reliably advance animation frame clocks); if it does not, the test drives the real `after-paint`
  path rather than a test-only release seam, or it adds exactly one counted, justified seam. A second instance counts `changed` and the keyfile's
  identity facts. With a fresh profile, it asserts zero writes before the first frame, exactly one
  coalesced flush afterwards, and persisted values equal to the pure oracle's. It also covers resize
  bursts producing at most one write per persistence window, and exact size and maximized state after
  `close_request`.
- **Measured lane.** C0's `make responsiveness-smoke` if it has landed, otherwise `make
  performance-smoke`. C0's `baseline` case already constructs a window on an empty and a small
  document, and C0 excludes construction from its budget verdict; C4 extends that case (it does not
  add a parallel one) to all four profile and backend combinations,
  reporting spans, the build profile, the backend, `btrfs-stall-report --no-below` for the run window,
  and an assertion against B_settle on the release build.

## Risks / Trade-offs

- **The settings file can lose the last state on a crash.** A crash between a toggle and the flush
  loses at most one persistence window of shell-layout preference changes. → These are layout
  preferences, not user content. Close flushes synchronously. The window is short. The behaviour is
  documented in `AGENTS.md`.
- **G3 shows backend lock contention.** → Fallback D3.2, measured. If it is still over budget, it
  escalates to Open Question 1 as a blocker, never to accepted debt.
- **Staged construction breaks tests or automation that assume eagerly built children.** → Readiness
  blockers plus the `*_ensure_constructed` entries. Widget tests use the shared readiness waits.
- **The measurement is noisy on this host** (btrfs flushoncommit). → The PR gate counts writes, which is
  deterministic. Time is asserted only on the measured lane, with stall-report evidence. A flake is a
  blocker to explain, not to retry.
- **Coordination with C0's constants and ledger ids.** → C4 is applied after C0 (stream order). Tasks
  start by checking C0's constant module and the last ledger id.

## Migration Plan

No persisted format changes. The migration flag and its five keys keep their meaning. Only the timing
and thread of the write change. Rollback means reverting the change. A profile already migrated by the
new code is indistinguishable from one migrated by the old code.

## Open Questions

1. **dconf access for the Flatpak.** If the probes and the fallback still cannot keep the keyfile flush
   within B_settle, should the Flatpak gain dconf access (`--filesystem=xdg-run/dconf`,
   `--filesystem=~/.config/dconf:ro`, `--talk-name=ca.desrt.dconf`,
   `--env=DCONF_USER_CONFIG_DIR=.config/dconf`)? That changes the permission posture, and existing
   keyfile settings would need a one-time import. This is a maintainer decision.
2. **Persistence window length** (working value 500 ms). Should it equal the session debounce, or be
   longer for resize bursts?
3. **The budget for the toolkit startup span.** `gtk_init`'s synchronous portal check is outside
   LushText's control. Should it be reported upstream (GTK), and should the reference lane provide a
   portal so that the span reflects production?
4. **Whether Preferences binds move to the writer** if task 3.5 measures one toggle above B_interact.
   That would delay their effect on open editors until the flush.
