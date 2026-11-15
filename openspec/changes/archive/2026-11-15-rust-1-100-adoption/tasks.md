Rules for every task: CPUs 8-23 (`taskset -c 8-23`). Every heavy command runs through `scripts/heavy.sh [--mem <cap>] -- <cmd>`, one command per call, and the widget runner uses `--mem none`. Run headless only. Stay inside this worktree and `.claude/handoffs/rust-1-100-adoption/`. Evidence goes in `openspec/changes/rust-1-100-adoption/evidence/`. Never wait for, poll or rerun GitHub CI.

## 1. A8, failing-first: the rename misclassification

- [x] 1.1 Give the **existing** `cfg(test)` parent-sync injection `fail_next_parent_sync_for_test` (both declarations: `services/durable_write.rs` and its `services/filesystem/write.rs` re-export) a `std::io::ErrorKind` parameter. Store the kind in the thread-local instead of a `bool`. Update its current callers (the `durable_write.rs` test, `local_history_service.rs`, `content_search/replace.rs`) to pass `ErrorKind::Other`, keeping the message. Do **not** add a second `pub`/`pub(crate)` `*_for_test`: the ratchet counts `#[cfg(test)]` declarations and stands at 171 of 171 (design D2). A further hook used only in `services` is `pub(in crate::services)`. Run `make check-workflow-boundaries` and confirm the count is still 171.
- [x] 1.2 Add failing-first unit tests in `services/filesystem/write.rs` (or `durable_write.rs` tests) and record their failure output in `evidence/a8-failing-first.md`:
  - `rename_durable_no_replace_or_checked` with an `Unsupported` parent-sync failure after a successful rename returns an after-rename failure, not `AlreadyExists`, with the file at the destination and the source gone;
  - the same with an `EIO`-like (`Other`) kind;
  - a fallback-path case (rename flag unsupported, forced through a `pub(in crate::services)` `cfg(test)` hook on `rename_no_replace`, or by calling the fallback leg directly) whose sync fails is after-rename;
  - an existing destination is still refused with `AlreadyExists` and left untouched (regression).
- [x] 1.3 Add a failing-first unit test for `ui/sidebar/workspace_section/file_execution.rs::rename_target_guarded`, run synchronously on the test thread with the injection: an after-rename sync failure yields `Ok(RenameDurability::Unconfirmed(_))`, never `Refused(DestinationExists)`. Cover both error kinds.
- [x] 1.4 Add a failing-first unit test in `services/draft_service/set_aside.rs`: a move whose directory sync fails with an `Unsupported` kind places the body under the first free name and returns it, instead of trying the next name.

## 2. A8, the fix (design D1, D2)

- [x] 2.1 `services/filesystem/write_protocol.rs`: `RenameAction::Finish(bool)` → `Finish(<class>)` (reuse `WriteClass` or add a sibling `RenameClass`; record the choice in the module doc). Update the existing unit tests and `a_completed_rename_synced_every_directory_it_mutated`.
- [x] 2.2 Add the Kani harness `every_rename_classification_describes_the_namespace` (name may be adjusted) to `write_protocol/kani_proofs.rs` per design D1, with `kani::cover!` for each class. Register it in `scripts/kani-shards.py` (Oracle `write_protocol.rs`, shard `core-journal-and-write`), then run `make check-kani-shards`.
- [x] 2.3 `services/durable_write.rs`: `run_rename_protocol` returns `Result<(), DurableWriteError>` by mapping the core's class one-to-one, and `rename_durable` keeps `io::Result<()>` through `into_io_error`. `rename_durable_no_replace` returns the classified error. Update the rustdoc.
- [x] 2.4 `services/filesystem/write.rs`: `rename_durable_no_replace_or_checked` returns `Result<(), DurableWriteError>`, falls back only on `BeforeRename` with kind `Unsupported`, and classifies the fallback leg the same way. Update `sys::rename_no_replace`'s and the wrappers' rustdoc (the "Errors" sections).
- [x] 2.5 `ui/sidebar/policy.rs`: add the pure durability-warning decision (design D2) with unit tests (confirmed → no warning; unconfirmed → warning naming the new name). No GTK imports.
- [x] 2.6 `ui/sidebar/workspace_section/file_execution.rs`: `rename_target_guarded` returns `Result<RenameDurability, RenameFailure>`. The completion has **one** `Ok(durability)` arm whose body is the existing success branch (design D2: the sidebar scenarios are proved by this structure plus task 1.3). After it, the arm emits the policy's warning through `section.emit_message` and logs with `tracing::warn!`. The `AlreadyExists` → `Refused` mapping applies only to `BeforeRename(AlreadyExists)`. Run the `data-safety` and `lushtext-workflow` skills' guidance on the touched files.
- [x] 2.7 `services/draft_service/set_aside.rs`: an after-rename result counts as placed (warn and `record_placement`). Keep `took_effect` only where it is still needed (the copy path) or replace it with the typed class, and say which in the code comment.
- [x] 2.8 Rerun tasks 1.2–1.4 and confirm they pass. Append the passing output to `evidence/a8-failing-first.md`.
- [x] 2.9 Re-derive the sidebar workflow's `WFR-*` row in `docs/workflow-readability-matrix.md` (pure-policy consumers, seam counts; row-scoped, excluding `#[cfg(test)]`) per `.agents/rules/workflow-convention.md`, then run `make check-workflow-boundaries`.

## 3. A1, the Clippy 1.100 sweep (design D3)

- [x] 3.1 Run `RUSTUP_TOOLCHAIN=1.100.0 scripts/heavy.sh -- cargo clippy --workspace --all-targets --all-features --locked --message-format=json -- --cap-lints warn` and write the deduplicated per-lint, per-crate counts and sites to `evidence/clippy-1.100-workspace.txt`. Intersect the sites with every `ORACLES` key in `scripts/kani-shards.py`, for **both** `lushtext-core` and `gtk-lush-widgets`, and record the result. A site in an oracle file adds that file's shard to task 7.4.
- [x] 3.2 Fix every site with MSRV-compatible APIs (`map_or_default`, the `needless_bool` guard, and anything else 1.100 reports). A site with no ≤ 1.98 fix gets a narrow `#[expect(..., reason = "...")]` and an entry in the evidence file. Leave `manual_bit_width` in `model/responsiveness_budget.rs` alone (declined).
- [x] 3.3 Prove the sweep clean: `RUSTUP_TOOLCHAIN=1.100.0 scripts/heavy.sh -- cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0, then `scripts/heavy.sh -- make check` (1.98.0) is clean. Rerun 3.3 after group 6, because the dependency refresh can move sites.

## 4. A7, A4, A3: policy and tooling (design D4, D5)

- [x] 4.1 `.agents/rules/build.md` item 5: add the version policy (never 1.99.0, the 1.99.1 floor, the missing 1.98.1 fix, and that the pin waits if Flathub ships 1.99.0), and name the advisory next-stable workflow as an explicit exception next to the nightly lanes.
- [x] 4.2 `.github/workflows/ci.yml` **and** `.github/workflows/mutation-testing.yml`: `CARGO_NEXTEST_VERSION` → `0.9.151`, the same in both. `build.md` states the ≥ 0.9.143 floor and why. Do **not** upgrade the local `~/.cargo/bin/cargo-nextest` (design D5). Record the upgrade command in the programme record (task 8.2) as an end-of-programme step for the maintainer.
- [x] 4.3 `scripts/lint-advisory.py`: make `GENERATED_OUT_DIR` accept `/build/<pkg>-<hash>/out/` and `/build/<pkg>/<hash>/out/`, and add a self-check of both path forms (and one non-generated path) that runs with `make lint-advisory`. Run `scripts/heavy.sh -- make lint-advisory`.
- [x] 4.4 `workspace-hack/Cargo.toml`: add an empty `[lints]` table outside the hakari markers. Run `cargo hakari generate` and `cargo hakari verify`, and confirm the table survives (also after group 6).
- [x] 4.5 New `.github/workflows/next-stable.yml` per design D4 (weekly `schedule` + `workflow_dispatch`, the Fedora 45 container and packages of the Lint job, `dtolnay/rust-toolchain@stable` with clippy, `rustc +stable --version`, `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings`, `timeout-minutes`). Run `make check-workflow-timeouts` and any workflow policy check in `make check-policy`. Do not dispatch it.

## 5. In-tab Replace All on the safe binding (design D7), applied after group 6's sourceview5 step

- [x] 5.1 `ui/search_bar/execution.rs`: replace the FFI block with `context.replace_all(replace)`, so no `unsafe` is left in the search bar. Keep the `Err` arm, which logs the `GError` at error level and never unwraps, and keep `report_match_state` unconditional. Rewrite the rustdoc: 0.11.3 returns the count; a zero count is success; the `GtkEntry` bound (65,535 characters, each insertion capped at 16 KiB by `entry_paste_cap`) makes the binding's `as i32` safe.
- [x] 5.2 Extend `test_replace_all_with_no_match_is_a_no_op` (`background_tab_layout.rs`) with an assertion that the match counter reports zero matches after the call; the scenario requires it.
- [x] 5.3 Probe whether GtkSourceView returns a `GError` from `replace_all` for a malformed regex replacement (regular expressions on, a replacement with a trailing `\` or another invalid escape).
  - If it does, add a widget test beside the no-match test. It asserts the document is unchanged, nothing panics, and the counter refreshes. It must stay clean under `scripts/run-widget-tests.sh`'s warning scan.
  - If it does not, record in the evidence that the arm has no driver.
- [x] 5.4 Run the widget tests `test_replace_all_with_no_match_is_a_no_op`, `test_replace_all_right_after_a_paced_reattach_replaces_every_match` (`background_tab_layout.rs`), the minimap replace-all test (`minimap_markers.rs`) and any test from 5.3. Run them in isolation through their shards (`make test-widget-shard WIDGET_SHARD=<shard>` via `heavy.sh --mem none`).
- [x] 5.5 Update `crates/lushtext-core/src/ui/editor_page/AGENTS.md` (the FFI sentence) and `docs/upstream-reports.md` (sourceview5-rs#19: released in 0.11.3 on 2026-11-14 and adopted, workaround removed).

## 6. A6, the cooldown refresh and sourceview5 0.11.3 (design D6)

- [x] 6.1 Add `make update-deps-cooldown` (and its `help` line) running design D6's five steps. The last step is the full `cargo deny check advisories bans sources licenses`. It fails with a clear message when `cargo +1.100.0` is unavailable. Document it in `build.md`'s Flatpak "Update chain" line and a new cooldown paragraph: environment variables only, `cargo install` ignores the cooldown, how to record an exception, the duplicate-skip policy, and the licence/advisory/source rule (design D6).
- [x] 6.2 Add the version-scoped `skip` entries to `deny.toml` **before** the first run: `{ crate = "syn@2", reason = … }` and `{ crate = "system-deps@7", reason = … }`. Each reason names the lagging crates and its removal condition (design D6). The local cargo-deny 0.19.5 accepted this form at the spec review.
- [x] 6.3 Run the whole `scripts/heavy.sh -- make update-deps-cooldown` with the **current** `sourceview5 = "0.11"` floor. It must exit 0. A failure is fixed per design D6:
  - another upstream-structural duplicate gets a skip or a hold;
  - a new licence, advisory or source follows D6's rule.
  
  Fix the failure and rerun the target until it passes once, end to end. Save `cargo update`'s summary and the deny result to `evidence/cooldown-refresh.txt`.
- [x] 6.4 Raise the floor in the root `Cargo.toml` to `sourceview5 = { version = "0.11.3", features = ["v5_18", "gtk_v4_18"] }` (the `gtk_v4_18` feature was found necessary at task 6.3: see `evidence/cooldown-refresh.txt`), then run `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +1.100.0 update -p sourceview5 -p sourceview5-sys --precise 0.11.3` without the cooldown. Record the exception (reason, versions, command, expiry 2026-11-21 08:57 UTC) in `build.md`'s cooldown paragraph.
- [x] 6.5 Run `cargo hakari generate`, `cargo hakari verify`, `make cargo-sources` (regenerates `build-aux/cargo-sources.json`), `scripts/heavy.sh -- cargo +1.98.0 check --workspace --all-targets --all-features --locked` and the full `cargo deny check advisories bans sources licenses`, one command per call. Confirm that the `[lints]` table in `workspace-hack/Cargo.toml` survived (task 4.4). Append the results to `evidence/cooldown-refresh.txt`.
- [x] 6.6 Side lockfiles (design D6):
  - **`fuzz/Cargo.lock` is mandatory.** Run step 1's cooldown command in `fuzz/` during task 6.3, **before** task 6.4 raises the floor (the cooldown refuses 0.11.3 once the floor is `^0.11.3`); after 6.4, run only the same `--precise 0.11.3` exception in `fuzz/`. Prove consistency with `cargo +1.100.0 metadata --locked --format-version 1 --manifest-path fuzz/Cargo.toml > /dev/null`.
  - Refresh `fixtures/gtk-lush-adoption/stock-settle/Cargo.lock` with step 1's command, or record its deferral and reason.
  - Leave `formal/evaluation/stateright/Cargo.lock` alone and record why: it is disposable, local-only evaluation tooling.
  
  Record all three in `evidence/cooldown-refresh.txt`.
- [x] 6.7 Run `scripts/heavy.sh -- make gtk-lush-msrv` and `scripts/heavy.sh --mem 16G -- cargo kani -p gtk-lush-widgets --target-dir target/kani --only-codegen` (design D3: the D1 shard does not compile this crate). Then run `make update-deps-cooldown` once more, expecting it to fail at step 1 on the sourceview5 floor. Record that failure's message as the documented behaviour until the expiry.

## 7. Verification

- [x] 7.1 Run `scripts/heavy.sh -- make check` and `scripts/heavy.sh -- make check-policy`, then repeat task 3.3's 1.100 Clippy run on the final tree.
- [x] 7.2 Run `scripts/heavy.sh --mem none -- make test` (unit, integration, every widget shard in parallel), then `scripts/heavy.sh -- make test-prop` and `scripts/heavy.sh -- make fuzz-corpus-replay` (always: it runs in the main workspace, whose graph this change refreshes). Any `FLAKY:` is a blocker under `preexisting-blockers.md`. The exception is the two tests PR #99 owns (`test_autosave_during_a_local_history_restore_keeps_the_previous_draft`, the sidebar directory-retention test): if those flake, merge `origin/main` after #99 lands and rerun rather than fixing them here.
- [x] 7.3 Run `scripts/heavy.sh --mem none -- make gtk-axioms` and `make check-gtk-axioms` as behaviour-preservation evidence through the refreshed bindings. This is **not** `build.md`'s toolkit-update procedure: no C library changes (design D6). So no ledger "Verified against" cell changes, and this change adds no axiom. Record the printed gtk/adw versions and the binding versions (gtk4, glib, sourceview5) in the evidence.
- [x] 7.4 Run the Kani shard `core-journal-and-write` **once**, detached, after the final A8 code: `setsid nohup scripts/heavy.sh -- make kani KANI_SHARD=core-journal-and-write`. Record the result and the new harness's measured seconds in `scripts/kani-shards.py`. Read the log and confirm that every `kani::cover!` of the new harness is SATISFIED: Kani does not fail a harness on an unsatisfied cover (design D1). No other shard reruns unless task 3.1 found a site in an oracle file, in which case run that shard once the same way.
- [x] 7.5 Run `scripts/heavy.sh -- make gtk-lush-doctests` and `scripts/heavy.sh -- make gtk-lush-msrv`, which exercise the Clippy fixes in GTK Lush on 1.98.0.
- [x] 7.6 Run `scripts/heavy.sh --mem none -- make flatpak` (Rust 1.98.0 extension, offline vendoring from the regenerated `cargo-sources.json`). That proves the vendoring matches the refreshed graph. If the host cannot build the Flatpak, record why and run the release helper's cargo-sources freshness check instead.
- [x] 7.7 Use no responsiveness lane and make no timing measurement. No hot path changes and no performance claim is made (design "Measured baseline"). Say so in the PR description.

## 8. Documentation and records

- [x] 8.1 Check every file `.agents/rules/documentation.md` lists: `README.md` (build commands: `make update-deps-cooldown`, if the README lists maintenance targets), `AGENTS.md` (Build Commands list, the Async Save / filesystem-boundary design decision if it describes the no-replace fallback), `.agents/rules/build.md` (tasks 4.1, 4.2, 6.1, 6.4), `docs/workflow-readability-matrix.md` (task 2.9), and `docs/upstream-reports.md` (task 5.5). Accessibility docs only if the new sidebar warning needs an announcement row (it reuses the section's existing message path).
- [x] 8.2 `docs/next/long-line-documents.md`: add a short `RA` record of this change (what landed, the deferred B items still gated on the Flathub extension and Kani, the cooldown exception and its expiry, and the deferred local `cargo install --locked cargo-nextest --version 0.9.151` as an end-of-programme step for the maintainer). Update `docs/next/formal-verification.md` if it lists the write-protocol harnesses.
- [x] 8.3 Run `make check-agent-docs` and `openspec validate rust-1-100-adoption --strict`.
- [x] 8.4 Merge `origin/main` into the branch if it moved (never rebase), rerun the gates the merge touches, and commit in conventional, signed commits staged by name.
