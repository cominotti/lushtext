# Tasks: Cold-Path Ergonomics and Shared Metadata

Kani: none (no proven module touched — `SharedPath`/`SharedStr` stay isolated from all Kani harnesses, and `paragraph_boundary::DocumentProfile`'s `Arc<FoldPlan>` stays unreached by Kani; `scripts/check-arc-safety.py` enforces zero `Arc` in Kani cores and `ui/**/policy.rs`).
Timing: `Verdict class: non-inferiority` on smoke cases `baseline/small/steady-populated`, `recovery/small-drafts`, `save/during-validation` and Criterion groups `scan_directory`, `tree_population`, `file_index_scale`, `file_index_search`, `end_to_end_boundedness`, `transient_file_load` via `scripts/measure.sh`.

## 1. Failing-first Arc-safety gate & Two-Zone Rust Rule

- [x] 1.1 Write `scripts/check-arc-safety.py` with `--self-test` proving failure on (a) naked `Arc<String>` / `Arc<Vec<u8>>` in production code, (b) `Arc` inside `ui/**/policy.rs` or a Kani-checked core, and (c) cold `<F: Fn*>` in `lushtext-ui` exceeding the ratcheted ceiling; verify the real scan flags the 6 `Arc<String>` sites in `install_execution.rs` and 85 UI generic-`Fn` functions.
- [x] 1.2 Wire `check-arc-safety` into `Makefile` (`check-policy`) and `.github/workflows/ci.yml`, codify the Two-Zone Rust Rule in `.agents/rules/rust.md` and `AGENTS.md`, run `make check-agent-docs`, create a signed commit (`git commit -S`), and push the branch.

## 2. `SharedPath`/`SharedStr` & `Arc<DisposalOwned<String>>` safety fix

- [x] 2.1 Implement `SharedPath(Arc<Path>)` and `SharedStr(Arc<str>)` in `crates/lushtext-model/src/values/shared.rs` with `Borrow`, `Deref`, `AsRef`, `Display`, `From`, and transparent serde support plus unit tests; verify `cargo test -p lushtext-model` and `make check-kani-shards` pass.
- [x] 2.2 Write a failing-first test verifying that closing a folded `NoteBody` in `ui/window/notes/install_execution.rs` retires the body through `DisposalOwned<String>` off the GTK thread rather than dropping a naked `Arc<String>`, then implement `SurfaceText` for `DisposalOwned<String>`, wire `DisposalAdmission` in `NoteBody::install` (with safe fallback when full), and migrate all 6 `Arc<String>` sites in `install_execution.rs` to `Arc<DisposalOwned<String>>`; verify the test passes.
- [x] 2.3 Adopt `SharedPath`/`SharedStr` across the high-clone cold UI/worker hotspots (`scan_execution.rs`, `folder_execution.rs`, `file_execution.rs`, `context_menus.rs`, `peek_execution.rs`, `dialogs.rs`, `notes/editor_execution.rs`, `notes/browser.rs`, `FileOperationTicket`, `SidebarFileRowStateSnapshot`), unify `model/content_search.rs` metadata, and replace double-indirected `Arc<PathBuf>` in `lushtext-services/src/palette/index/` (`arena.rs`, `mod.rs`, `mutation.rs`, `owned.rs`, `paths.rs`, `reference.rs`, preserving `folder_bytes` layout accounting); verify `make test-unit` and affected sidebar/notes widget shards pass, create a signed commit (`git commit -S`), and push the branch.

## 3. Boxed cold state, cold `<F: Fn*>` cleanup & final verification

- [x] 3.1 Box large cold optional state (`Option<Box<T>>`) in widget state and cold enum variants, and convert remaining cold generic `<F: Fn*>` UI functions (`save_file_async`, `snapshot_buffer_text_async`, etc.) to `Box<dyn Fn...>` or thin `impl FnOnce` -> non-generic `fn inner` wrappers (`<= 25` ratcheted); verify `make check`, `make check-policy`, `make test-unit`, and widget tests pass.
- [x] 3.2 Run the pre-registered non-inferiority timing verification via `scripts/measure.sh` / `make responsiveness-smoke` and confirm all touched cases pass within the 500 µs noise band and `B_INTERACT_US`/`B_SETTLE_US`.
- [x] 3.3 Update `docs/next/idiomatic-rust-hexagonal-programme.md` with programme completion outcomes across all 5 changes, run `openspec validate cold-path-ergonomics-and-shared-metadata --strict`, create a signed commit (`git commit -S`), and push the branch.
