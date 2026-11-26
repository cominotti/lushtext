## ADDED Requirements

### Requirement: Bounded immutable UI and worker metadata uses SharedPath and SharedStr
`lushtext-model::values::shared` SHALL provide `SharedPath(Arc<Path>)` and `SharedStr(Arc<str>)` (outside Kani-checked modules) for immutable paths, IDs, and labels crossing GTK closures and `spawn_blocking_then` handoffs without repeated `PathBuf`/`String` allocations or `'a` lifetime propagation.

#### Scenario: Ticket and snapshot paths clone in O(1) without lifetimes
- **WHEN** `FileOperationTicket`, `SidebarFileRowStateSnapshot`, or sidebar/dialog worker handoffs copy a file path or ID across closures
- **THEN** cloning `SharedPath` or `SharedStr` increments an atomic reference count rather than allocating and copying path bytes

#### Scenario: Hot arena indices keep compact ordinals and borrowed views
- **WHEN** `services/palette/index` or `services/palette/fuzzy` ranks or stores thousands of file entries
- **THEN** it retains `FileOrdinal(u32)` and borrowed `'a` views rather than substituting `SharedPath`

### Requirement: Cold UI handlers prefer concrete types, Box<dyn Fn>, and boxed cold state
Cold UI orchestration helpers in `lushtext-ui` SHALL avoid heavy generic `<F: Fn*>` monomorphization (using `Box<dyn Fn...>` or a thin generic facade delegating to non-generic `fn inner(..., Box<dyn Fn...>)`, ratcheted to `<= 25` cold generic `Fn` signatures) and SHALL box large cold optional state (`Option<Box<T>>`).

#### Scenario: Cold UI callback helper avoids monomorphization bloat without call-site clutter
- **WHEN** an infrequent UI workflow helper such as `save_file_async` or `snapshot_buffer_text_async` accepts a completion callback
- **THEN** either it takes `Box<dyn FnOnce(...)>` directly or delegates from a one-line `impl FnOnce` wrapper into a non-generic `fn inner` so monomorphization is eliminated without `Box::new` boilerplate at every caller

### Requirement: Two-zone Arc safety and Kani purity are gated with a self-test
`scripts/check-arc-safety.py` (with `--self-test`, run in `make check-policy`) SHALL forbid `Arc` in `ui/**/policy.rs` and Kani-checked modules (except documented `#[cfg(not(kani))]` or non-Kani profile attachments), forbid naked `Arc<String>` / `Arc<Vec<u8>>`, and ratchet cold `<F: Fn*>` functions in `lushtext-ui`.

#### Scenario: Self-test catches Arc in policy and naked Arc<String>
- **WHEN** `python3 scripts/check-arc-safety.py --self-test` runs
- **THEN** it proves failure on `Arc` inside a `ui/**/policy.rs` fixture, naked `Arc<String>`/`Arc<Vec<u8>>` in UI code, and excess cold generic `<F: Fn*>` signatures
