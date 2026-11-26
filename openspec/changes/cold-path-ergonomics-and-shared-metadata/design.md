# Design: Cold-Path Ergonomics and Shared Metadata

## Context
Matklad's *"Rust's Ugly Syntax"* shows that dense Rust syntax exists to represent real semantic constraints (zero-cost borrowing, pinned layouts, monomorphization control). Outside hot inner loops, however, paying maximum semantic complexity everywhere imposes avoidable cognitive load. In LushText's UI/orchestration tier, 9 files each perform 15–40 `.clone()` calls (mostly `PathBuf`/`String` metadata), 85 UI functions carry `<F: Fn*>` generics, and `ui/window/notes/install_execution.rs` holds 6 naked `Arc<String>` references for folded note bodies above `L_PROTECT_EDIT` without `DisposalOwned` drop protection.

## Goals / Non-Goals
**Goals:**
- Establish the Two-Zone Rust Rule in `.agents/rules/rust.md` (Hot/Kani/Arena vs Cold/Orchestration).
- Add `SharedPath(Arc<Path>)` and `SharedStr(Arc<str>)` in `lushtext-model::values::shared` for cheap-clone immutable metadata across cold UI/worker seams.
- Upgrade `ui/window/notes/install_execution.rs` from naked `Arc<String>` to `Arc<DisposalOwned<String>>`.
- Box cold optional state (`Option<Box<T>>`) and convert cold `<F: Fn*>` UI helpers to `Box<dyn Fn...>` or matklad's concrete `fn inner` pattern.
- Enforce `Arc` safety and cold generic ratchets via `scripts/check-arc-safety.py --self-test`.

**Non-Goals:**
- Introducing `Arc` inside `services/palette/index`, `services/palette/fuzzy` (where `RetainedCandidate<'a>`'s 36-byte layout is `const assert!`-pinned), `ui/**/policy.rs`, or Kani proof cores.
- Wrapping bounded/small UI metadata in `DisposalOwned` (only document-sized payloads need off-GTK retirement).

## Decisions

### D1. `SharedPath(Arc<Path>)` and `SharedStr(Arc<str>)` in `lushtext-model::values::shared`
- **Decision**: Define thin `#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]` wrappers `SharedPath(Arc<Path>)` and `SharedStr(Arc<str>)` implementing `Deref`, `AsRef`, `Borrow<Path>`/`Borrow<str>`, `Display`, `From<&Path>`/`From<PathBuf>`/`From<&str>`/`From<String>`, and `serde::{Serialize, Deserialize}` (via `&Path`/`&str` transparently). Keep `values::shared` unreferenced by all `kani_proofs.rs` modules. Adopt `SharedPath`/`SharedStr` across UI/worker metadata, `model/content_search.rs`, and `services/palette/index/` (replacing 14+ double-indirected `Arc<PathBuf>` fields while updating `folder_bytes` layout accounting cleanly).
- **Rationale**: Makes `.clone()` across GTK closures and `spawn_blocking_then` O(1) without lifetimes (`'a`), removes `Arc<PathBuf>` double pointer chasing in the palette index, and keeps `HashMap`/`HashSet` lookups by `&Path`/`&str` allocation-free via `Borrow`.
- **Alternatives rejected**: Propagating `'a` lifetimes through async/UI state structs (incompatible with GTK closures and `'static` worker futures); raw `Arc<PathBuf>` (double indirection: `Arc` -> `PathBuf` -> heap buffer).

### D2. Protecting document-sized shared payloads with `Arc<DisposalOwned<T>>` and `SurfaceText`
- **Decision**: Audit all `Arc<String>` / `Arc<Vec<u8>>` uses; migrate `ui/window/notes/install_execution.rs`'s 6 naked `Arc<String>` sites (for folded notes exceeding `L_PROTECT_EDIT`) to `Arc<DisposalOwned<String>>`, matching `command_palette`'s proven `Arc<DisposalOwned<FileIndex>>`. Specifically: (a) implement `SurfaceText` for `DisposalOwned<String>` so `Arc::downgrade` coerces directly to `Weak<dyn SurfaceText>` for `FoldedTextSurface::set_folded_source`, and (b) admit the body through `ui::plain_disposal` admission in `NoteBody::install` (falling back safely to a bounded read-only notice without main-thread document drop if admission is temporarily full).
- **Rationale**: A bare `Arc<String>` deallocates on whichever thread drops the last strong reference—if the background worker finishes before the GTK `NoteBody` closes, dropping a multi-megabyte folded note runs on the GTK main thread and threatens `B_INTERACT_US`.
- **Alternatives rejected**: Leaving naked `Arc<String>` for notes (creates an unguarded off-contract exception when `command_palette` already proved `Arc<DisposalOwned<...>>`); dropping `FoldedTextSurface` sharing (breaks folded-note copy/render source).

### D3. Auditing `Arc<FoldPlan>` in `model/paragraph_boundary.rs`
- **Decision**: Check `paragraph_boundary::kani_proofs` interaction with `DocumentProfile::fold_plan: Option<Arc<FoldPlan>>`; keep `fold_plan` behind `#[cfg(not(kani))]` or verify that no Kani harness touches `Arc` allocation, so `scripts/check-arc-safety.py` enforces zero `Arc` execution inside Kani cores cleanly without false positives.
- **Rationale**: Ensures Kani/CBMC never models atomic heap allocations.
- **Alternatives rejected**: Exempting all of `lushtext-model` from the `Arc` safety check (would weaken Kani tractability).

### D4. Cold `<F: Fn*>` simplification (matklad `fn inner` pattern) & `Option<Box<T>>`
- **Decision**: After `idiomatic-rust-state-machines-and-c-ism-removal` eliminates most `connect_*` callback generics, convert remaining cold UI async helpers (`save_file_async`, `snapshot_buffer_text_async`, etc.) to `Box<dyn Fn...>` or a thin generic facade (`impl FnOnce(...) + 'static`) delegating immediately into a non-generic `fn inner(..., Box<dyn FnOnce(...)>)`, bringing cold UI generic-`Fn` functions down to `<= 25` (ratcheted); box large cold optional state (`Option<Box<ChunkedLoadInstall>>`, cold enum variants) to shrink stack frames.
- **Rationale**: Directly applies matklad's `std::fs::read` insight—keeping ergonomic closure calls at the call site without monomorphizing multi-hundred-line worker/GTK bodies per closure type.
- **Alternatives rejected**: Mandatory `Box<dyn Fn>` in hot iterators like `palette/fuzzy.rs` (would regress inner-loop performance); forcing `Box::new(|...|)` boilerplate on every caller of `save_file_async`.

## Budget Exceptions
None.

## GTK Axiom Ledger
None (preserves `B_INTERACT_US` / `B_SETTLE_US` and `DisposalOwned` off-GTK drop contracts).

## Timing
Verdict class: non-inferiority (touches `sidebar/workspace_section`, `window/notes/install_execution.rs`, `services/palette/index/`, and `window/dialogs.rs` metadata paths).
- **Pre-registered protocol** (per `.agents/rules/verification-runs.md`): run `scripts/measure.sh` / `make responsiveness-smoke` and `cargo bench` comparing `main` vs branch on literal smoke cases `baseline/small/steady-populated`, `recovery/small-drafts`, `save/during-validation`, and Criterion groups `scan_directory`, `tree_population`, `file_index_scale`, `file_index_search`, `end_to_end_boundedness`, `transient_file_load`.
- **Primary metrics**: p50/p95 dispatch latency (µs) against `B_INTERACT_US` / `B_SETTLE_US` and heap allocation counts where profiled.
- **Noise band**: 500 µs per dispatch; zero regressions over `B_INTERACT_US` or `B_SETTLE_US` where `main` passes.
