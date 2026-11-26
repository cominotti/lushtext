# Proposal: Cold-Path Ergonomics and Shared Metadata

## Why
As matklad's *"Rust's Ugly Syntax"* explains, dense Rust syntax (`'a`, slices, explicit generic bounds, single-owner moves) reflects semantic rigor needed in hot paths—not a mandate to pay that cognitive tax everywhere. In LushText's cold UI orchestration and worker handoffs, cloning `PathBuf`/`String` repeatedly and threading generic `<F: Fn...>` callbacks creates unnecessary clutter and allocations, while `ui/window/notes/install_execution.rs` uses naked `Arc<String>` for folded note bodies above `L_PROTECT_EDIT` without `DisposalOwned` off-GTK drop safety.

## Baseline
Measured on `main` (`3c442112`) across production code in `crates/lushtext-{model,services,core}/src`:
- High-clone UI files (`>= 15` `.clone()` calls): **9** files (`scan_execution.rs`: 40, `dialogs.rs`: 31, `notes/editor_execution.rs`: 29, `folder_execution.rs`: 25, `file_execution.rs`: 22, `context_menus.rs`: 21, `peek_execution.rs`: 16, `replace_execution.rs`: 15, `notes/browser.rs`: 15).
- Double-indirected `Arc<PathBuf>` in `lushtext-services/src/palette/index/`: **14+** occurrences across `arena.rs`, `mod.rs`, `mutation.rs`, `owned.rs`, `paths.rs`, and `reference.rs`, plus raw `Arc<Path>`/`Arc<str>` metadata in `lushtext-model/src/content_search.rs`.
- Cold UI generic `<F: Fn*>` functions: **85** functions under `src/ui/` (many retired by `idiomatic-rust-state-machines-and-c-ism-removal`; remaining async helpers like `save_file_async` and `snapshot_buffer_text_async` still monomorphize every closure).
- Naked `Arc<String>` in production UI: **6** occurrences in `ui/window/notes/install_execution.rs` (holding bodies above `L_PROTECT_EDIT` and coercing to `Weak<dyn SurfaceText>` without `DisposalOwned<String>`), compared to `command_palette`'s proven `Arc<DisposalOwned<FileIndex>>` (**6** occurrences).
- Kani-verified modules using `Arc`: `model/paragraph_boundary.rs` holds `fold_plan: Option<Arc<FoldPlan>>` on `DocumentProfile`.

## What Changes
- Codify the **Two-Zone Rust Rule** in `.agents/rules/rust.md`:
  - *Hot / Kani / Arena zone* (`lushtext-model` proven modules, `ui/**/policy.rs`, `text_scans`, `palette/index`, `palette/fuzzy`, `content_search` inner loops): strict zero-cost semantics (slices, borrowed views, `Copy` value types, `DisposalOwned<T>`, zero `Arc` in Kani cores, zero `Box<dyn>`).
  - *Cold / Orchestration / UI zone*: concrete parameter types (`&Path` / `SharedPath` over `<P: AsRef<Path>>`), cheap-clone immutable metadata (`SharedPath(Arc<Path>)` / `SharedStr(Arc<str>)`), `Option<Box<T>>` for large/rare widget state, and `Box<dyn Fn...>` (or matklad's thin `impl FnOnce` facade calling a non-generic `fn inner(..., Box<dyn FnOnce(...)>)`) instead of monomorphizing generic `<F: Fn*>` bodies.
- Introduce `SharedPath(Arc<Path>)` and `SharedStr(Arc<str>)` in `lushtext-model::values::shared` (explicitly isolated from all Kani harnesses) and adopt them across `FileOperationTicket`, `SidebarFileRowStateSnapshot`, sidebar/notes/dialog metadata handoffs, `spawn_blocking_then` payloads, `content_search.rs` metadata, and `services/palette/index/` (eliminating `Arc<PathBuf>` double indirection).
- Migrate `ui/window/notes/install_execution.rs`'s **6** naked `Arc<String>` references to `Arc<DisposalOwned<String>>` by implementing `SurfaceText` for `DisposalOwned<String>` and wiring `DisposalAdmission` (matching `command_palette`'s `Arc<DisposalOwned<...>>` contract) so document-sized folded note bodies never drop on the GTK main thread.
- Box large cold optional state (`Option<Box<T>>`) in widget state and enum variants, and simplify cold generic `<F: Fn*>` UI functions using `Box<dyn Fn...>` or thin `impl FnOnce` -> `fn inner` wrappers.
- Add `scripts/check-arc-safety.py` (`--self-test`, wired into `make check-policy`): hard-bans naked `Arc<String>` / `Arc<Vec<u8>>` and `Arc` inside `ui/**/policy.rs` and Kani-checked code, and ratchets cold generic `<F: Fn*>` functions in `lushtext-ui`.

## Non-goals
- Replacing `FileOrdinal(u32)` or borrowed `'a` views inside `services/palette/index` or `services/palette/fuzzy` (hot arena/ranking paths keep their pinned 36-byte cache layout).
- Wrapping `String` / `Vec<u8>` document text in `Arc` without `DisposalOwned<T>`.
- Changing `command_palette`'s already-proven `Arc<DisposalOwned<FileIndex>>` pattern.

## User-visible impact
Zero user-visible behavioral loss compared with `main`. Eliminates main-thread drop risk for folded large notes in `install_execution.rs` and reduces repeated path/string heap allocations across sidebar/dialog workflows.

## Capabilities

### New Capabilities
- `cold-path-ergonomics-and-shared-metadata`: Distinguish hot/Kani/arena Rust from cold UI/orchestration ergonomics with `SharedPath`/`SharedStr` metadata, boxed cold state, and mechanical guards protecting `DisposalOwned<T>` and Kani purity.

### Modified Capabilities
- `typed-payload-ownership-contracts`: Require shared `Arc` values to carry either bounded immutable metadata (`SharedPath`/`SharedStr`) or `DisposalOwned<T>` for document-sized payloads, and keep `Arc` out of `ui/**/policy.rs` and Kani-verified cores.

## Impact
- **Code**: `crates/lushtext-model/src/values/shared.rs`, `crates/lushtext-services/`, `crates/lushtext-ui/src/ui/` (`sidebar/workspace_section/`, `window/notes/install_execution.rs`, `window/dialogs.rs`, `buffer_snapshot.rs`, cold UI handlers).
- **Gates**: `scripts/check-arc-safety.py`, `Makefile`, `.github/workflows/ci.yml`.
- **Docs**: `.agents/rules/rust.md`, `AGENTS.md`, `docs/next/idiomatic-rust-hexagonal-programme.md`.
