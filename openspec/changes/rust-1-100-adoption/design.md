## Context

The investigation (`.claude/handoffs/rust-1-100-adoption/proposal.md`, with its independent review) classified every Rust 1.99/1.100 item. The maintainer chose its "recommended package": A8, A1, A7, A2, A4, A3 and A6. A5 (`CARGO_INCREMENTAL=0` in `heavy.sh`) is **not** part of it; it belongs to the parallel `reduce-build-disk-writes` change, which owns the build-directory scripts. The maintainer also added the sourceview5 0.11.3 bump, because it regenerates the same lockfile, `workspace-hack` and `cargo-sources.json`.

Constraints that do not move:

- **Pin and MSRV stay 1.98.0.** `rust-stable//26.08` ships 1.98.0, and `build.md` item 5 requires tested = shipped. Every fix in this change must compile on 1.98.0 and on Kani 0.68.0's `nightly-2026-08-21`. `cargo kani -p <package>` compiles that package's whole library, and each shard names one package (`scripts/kani-shards.py` `shard_command`). A `lushtext-core` shard therefore never compiles `gtk-lush-widgets`' `cfg(kani)` code.
- **`clippy::incompatible_msrv`** rejects post-1.98 APIs outside test code. Clippy 1.100 is MSRV-aware through `rust-version`, so its suggestions stay at or below 1.98.
- **Parallel runs** (`harden-flatpak-sdk-update-ci`, `reduce-build-disk-writes`, `slice-large-directory-collapse`, `stream-replace-all`, `scale-file-index`, `restore-main-ci` / PR #99) merge `main` after this lands. This change goes first and moves briskly.

## Measured baseline (2026-11-14, `origin/main` `72c039ef`)

| Item | Measurement | How |
| --- | --- | --- |
| Clippy 1.100 on `lushtext-core` (all features, all targets) | 32 `map_unwrap_or` (denied; lib 29 errors, lib test 31) + 1 `needless_bool`; 0 on 1.98.0 and 1.99.1 | investigation, `clippy-1.100.txt` |
| Clippy 1.100, other members | **unmeasured**; a regex estimate is about 23 more `map(..).unwrap_or_default()` sites (`crates/lushtext` 11, `gtk-lush-axioms` 8, `cargo-gtk-proof` 2, adoption lab 1, `gtk-lush-widgets` 1). The regex undercounted core by 5, so expect 55–65 in total | investigation review |
| Clippy sites in Kani oracle files | **0** of the 32 core sites lie in a file listed in `scripts/kani-shards.py` (only the declined `manual_bit_width` would) | intersection of `clippy-1.100.txt` with the oracle keys |
| Cooldown refresh, dry run | `cargo +1.100.0 update` with `CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="7 days"` and `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`: "Locking 129 packages to highest Rust 1.98.0 compatible versions as of 7 days ago" | dry run in the worktree |
| Cooldown refresh, applied then reverted | `cargo deny check bans` **FAILED**: `syn` 2.0.119 + 3.0.9 (clap_derive 4.6.7 needs syn 3; tracing-attributes, zerocopy-derive and `workspace-hack` stay on 2), and `system-deps` 7.0.8 + 9.0.1 (gtk-rs-core `*-sys` 0.22.9 moved to 9 on 2026-08-30; libadwaita-sys 0.9.2 and sourceview5-sys 0.11.2/0.11.3 still need 7); the rest are `workspace-duplicate` warnings | applied in the worktree, `cargo deny --offline check bans`, `cargo tree -i`, then `git checkout Cargo.lock` |
| sourceview5 under the cooldown | held at 0.11.2 ("available: v0.11.3, published 15 hours ago"); `update -p sourceview5 --precise 0.11.3` under the cooldown: "version 0.11.3 is too new (published 15 hours ago, minimum age 7 days)" | dry runs |
| A lockfile already holding 0.11.3, floor `^0.11.3`, then a full cooldown run | fails: "failed to select a version for the requirement `sourceview5 = "^0.11.3"`". A full `update` re-resolves the lock, so Cargo's "unless already in `Cargo.lock`" clause does not keep it | dry run on a temporary lock |
| Per-package override | none. `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=fallback` is rejected ("could not load config key"); `=allow` drops the whole cooldown (it took glib 0.22.11, published 10 hours ago) | dry runs |
| sourceview5 0.11.0 → 0.11.3 public API diff | `SearchContext::replace_all` `Result<(), _>` → `Result<u32, _>`; `match_style()` and `FileExt::location()` now return `Option`; `FileLoader` builder gains `max_size`. LushText uses none except `replace_all`, which it reaches through FFI today | crate sources diffed |
| cargo-nextest releases | 0.9.152 (2026-11-13, inside the cooldown), **0.9.151** (2026-11-03), … 0.9.143 is the floor for layout v2 | crates.io API |
| Toolchains installed | 1.98.0, 1.98.1, 1.99.1, 1.100.0, Kani's `nightly-2026-08-21` | `rustup toolchain list` |
| Spec review, refreshed graph, rest of the deny policy | The cooldown refresh was re-applied on the old floor, before hakari. `cargo deny check advisories licenses sources` printed "advisories ok, licenses ok, sources ok". `bans` failed only on `syn` and `system-deps`. With `skip = [{ crate = "syn@2", … }, { crate = "system-deps@7", … }]`, it printed "bans ok". Cargo-deny is 0.19.5 locally; the CI pin 0.19.8 is unverified locally. Then `git checkout Cargo.lock deny.toml` | spec review, 2026-11-14 |
| `zlib-rs` 0.6.8 (licence `Zlib`, which the allow list lacks) | It enters `Cargo.lock` but lies outside cargo-deny's graph for the configured targets and features (`cargo tree -i zlib-rs --target all` matches nothing). `cargo-sources.json` will still vendor it | same |
| `*_for_test` ratchet | `count_for_test_declarations` counts **171** = the matrix ceiling 171. It counts every `pub`/`pub(crate) fn *_for_test`, `#[cfg(test)]` ones included (for example `fail_next_parent_sync_for_test` in `durable_write.rs` and `filesystem/write.rs`). `pub(in crate::services)` is not counted | `scripts/check-workflow-boundaries.py` |
| Directory sync errors | `sys::sync_dir_descriptor` returns the raw `openat`/`fsync` errno, so an `EOPNOTSUPP`/`ENOSYS` directory fsync reaches the fallback as `Unsupported`. `sys::rename_no_replace` maps `EINVAL`/`ENOSYS`/`EOPNOTSUPP` from `renameat2` to `Unsupported` | `services/filesystem/sys.rs` |
| Other `cargo-nextest` pins | `.github/workflows/mutation-testing.yml` also pins `CARGO_NEXTEST_VERSION: "0.9.137"`. `rust-linting-policy` requires every workflow that installs nextest to use the same version | `grep` |
| Side lockfiles and the sourceview5 floor | The fuzz crate depends on `lushtext-core` by path, which takes `sourceview5` from the root workspace table. `fuzz/Cargo.lock` holds sourceview5 **0.11.0**, so a `^0.11.3` floor leaves it inconsistent. The stock-settle fixture has no sourceview5. `make fuzz-corpus-replay` runs in the **main** workspace (`cargo test -p lushtext-core --features fuzzing`), not the fuzz crate | `fuzz/Cargo.toml`, `Makefile` |
| Clippy site in `gtk-lush-widgets` | The one regex hit is `examples/viewport_slice_bin.rs`, which Kani does not compile. The library has none | `grep` |
| sourceview5 0.11.3 `replace_all` source | `let replace_length = replace.len() as i32;`, then the FFI call; `Ok(count)` when the error is null | the 0.11.3 crate from crates.io |

No timing measurement is needed. No decision depends on a duration, and no hot path changes. The dependency refresh moves Rust bindings, not the C libraries the Flatpak and the host provide. This change makes no performance claim, so it runs no responsiveness lane case.

## Decisions

### D1. Classify the rename in the verified core, not in the shell (A8)

`durable-file-write-contract` requires the protocol, "the before- and after-rename classification … and durable rename", to live in the I/O-free core, and requires that "the shell adds no decisions". Today `RenameAction::Finish(bool)` carries no class, so a fix in `run_rename_protocol` that asks "which step failed" would put a decision in the shell. Instead:

- `RenameAction::Finish(bool)` becomes `RenameAction::Finish(RenameClass)`. Reuse `WriteClass` (`Success`, `BeforeRename`, `AfterRename`) if its doc text fits a rename; otherwise add a sibling `RenameClass` with the same three variants. The implementer picks whichever reads more clearly and records the choice. `Rename` failing → `BeforeRename`; any sync failing after a successful `Rename` → `AfterRename`.
- `run_rename_protocol` returns `Result<(), DurableWriteError>`. It maps the class and the last error one-to-one, so it adds no decision. `durable_write::rename_durable` keeps `io::Result<()>` through `DurableWriteError::into_io_error`, so its other callers are unchanged. The no-replace variant returns the classified error.
- `filesystem::write::rename_durable_no_replace_or_checked` returns `Result<(), DurableWriteError>`. The fallback runs only on `BeforeRename(e)` with `e.kind() == Unsupported`. `AlreadyExists` from either route is `BeforeRename(AlreadyExists)`. The fallback's `rename_durable` leg is classified by the same protocol, so it has no separate after-rename gap.

Rejected alternatives: (a) a dedicated error marker from `sys::rename_no_replace`, which is smaller but leaves the sidebar's general after-rename gap and still asks the shell to decide; (b) re-checking "target present, source gone" afterwards, which races with concurrent writers. That stat check stays only as `set_aside::took_effect` for the **copy** path, `atomic_replace`, which already returns its own `DurableWriteError`. The implementer may move set-aside to the typed class for both paths if that removes `took_effect` cleanly.

**Kani.** Add a harness to `write_protocol/kani_proofs.rs`, for example `every_rename_classification_describes_the_namespace`. Over every outcome sequence and both `cross_directory` values it checks: `BeforeRename` ⇒ the rename did not happen; `AfterRename` ⇒ it did, and some directory sync failed; `Success` ⇒ it did, and every mutated directory synced. Use `kani::cover!` for each class. Kani reports an unsatisfied cover without failing the harness, and `kani-shards.py` does not check covers. So the shard run's log is read for the new harness, and every cover must be SATISFIED; an unsatisfied one is a finding that must be fixed before sign-off. Register it in `scripts/kani-shards.py` (Oracle `write_protocol.rs`, shard `core-journal-and-write`, measured seconds), extend the existing `a_completed_rename_synced_every_directory_it_mutated` for the new `Finish` payload, and run the shard **once**, detached, after the final code (`.agents/rules/verification-runs.md`). It is a scheduled shard, not PR-gated.

### D2. The sidebar projects an after-rename result as a completed rename, decided by pure policy (A8)

`rename_target_guarded` returns `Result<RenameDurability, RenameFailure>`, where `RenameDurability` is `Confirmed` or `Unconfirmed(io::Error)`. A pure function in `ui/sidebar/policy.rs` decides the user-visible part: given the durability and the new name, it returns the optional warning text and its severity (Warning). Suggested wording: "Renamed to '{name}', but LushText could not confirm the change reached the disk". The final wording is decided at implementation against `.agents/rules/ui.md`'s message style. The GTK arm in `file_execution.rs` runs the existing success branch unchanged for both variants, then emits the warning. For a New File/Folder first name it runs the create callback, never the cleanup. `tracing::warn!` records the error.

No widget-level test seam is added. The parent-sync injection is `cfg(test)` and thread-local. Widget tests build `lushtext-core` with `test-utils`, not `cfg(test)`, and the sidebar rename runs on a `spawn_blocking_then` worker. A widget test would therefore need a new `test-utils` override, and the `*_for_test` ratchet is at its ceiling (171 of 171, measured). The GTK arm projects both durabilities through **one** `Ok(durability)` arm, whose body is the existing success branch, so the projection cannot diverge between them. The proof is:

- a core unit test of `rename_target_guarded` on the calling thread with the injection. It covers an `Unsupported` kind and an `EIO`-like kind: both return `Unconfirmed`, the file is at the new path, and no `DestinationExists` refusal is returned;
- a pure-policy unit test;
- the unchanged widget tests that cover the success branch;
- review of the single arm.

The sidebar scenarios in `durable-file-write-contract` are proved by that composition, and the spec says so.

**The injection keeps the seam count.** The ratchet counts `#[cfg(test)]` declarations too, so a new `fail_next_parent_sync_with_for_test` wrapper beside the existing one would add two `pub(crate)` declarations (`durable_write.rs` and its `filesystem/write.rs` re-export) and fail `make check-workflow-boundaries`. Instead, the **existing** `fail_next_parent_sync_for_test` (both declarations) takes a `std::io::ErrorKind`. Its current callers (a `durable_write.rs` test, `local_history_service.rs`, `content_search/replace.rs`, and the `filesystem/write.rs` wrapper's delegation) pass `ErrorKind::Other` and keep the message "injected parent directory sync failure". Any further hook used only inside `services` (for example a forced `Unsupported` from `rename_no_replace` for the fallback-leg test) is `pub(in crate::services)`, which the ratchet does not count. The count stays at 171.

The sidebar's `WFR-*` matrix row is re-derived for the new policy function and the changed seam.

### D3. Clippy sweep: measure workspace-wide first, fix with MSRV-safe APIs (A1)

Run `RUSTUP_TOOLCHAIN=1.100.0 scripts/heavy.sh -- cargo clippy --workspace --all-targets --all-features --locked -- --cap-lints warn` (with `--message-format=json`) and record the deduplicated counts per lint and per crate in `evidence/clippy-1.100-workspace.txt`. Fix every site. `map_or_default` is stable since 1.98.0. Anything 1.100 reports that has no ≤ 1.98 fix is decided per case (a narrow `#[expect(…, reason)]` when it is a false positive under the MSRV) and recorded. Done means: Clippy 1.100 with `-D warnings` exits 0 on the workspace, and `make check` (1.98.0) is clean. The pedantic `manual_bit_width` on the Kani-proved `histogram_bucket` is declined (investigation review: not enabled, and it would cost a shard rerun).

**Kani after A1.** No shard reruns for A1, **provided** task 3.1's workspace result shows no site in an oracle file of either Kani package. That has been measured for `lushtext-core` only, so it is re-checked against `scripts/kani-shards.py`'s `ORACLES` keys for `gtk-lush-widgets` too. A site in an oracle file means that file's shard reruns once, detached, after the final code. The D1 shard run compiles `lushtext-core` under Kani. It does **not** compile `gtk-lush-widgets`, whose library has no A1 site but whose dependencies (gtk4, glib) the D6 refresh moves. So the Kani compile of that crate is proved separately, without verification: `scripts/heavy.sh --mem 16G -- cargo kani -p gtk-lush-widgets --target-dir target/kani --only-codegen` (`--only-codegen` exists in Kani 0.68.0; the widgets shards carry no `kani_flags`). The PR-gated `widgets-geometry` shard is left to CI at the end of the programme.

### D4. Version policy and the advisory lane (A7, A2)

`build.md` item 5 gains: never adopt **1.99.0** (FFI `bool` return miscompile on x86_64, rust#163911); **1.99.1** is the floor of any 1.99 pin; the shipped 1.98.0 lacks the 1.98.1 boxed-async vtable fix (rust#161441, low exposure: no async services). If Flathub merges the 1.99.0 PR #553 to `26.08` as-is, the pin does **not** follow; LushText waits for a ≥ 1.99.1 extension.

A new `.github/workflows/next-stable.yml` runs weekly (`schedule`) and on `workflow_dispatch`, never on `pull_request` or `push`, so it cannot become a required check. It reuses the Lint job's Fedora 45 container and system packages. It installs `stable` with Clippy through `dtolnay/rust-toolchain@stable` and runs `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings`. The explicit `+stable` overrides `rust-toolchain.toml`, without which the job would silently test 1.98.0. It prints `rustc +stable --version` first so a run names what it tested. It has a `timeout-minutes` (`make check-workflow-timeouts`). Its failures are advisory: whoever reads it fixes churn early, like A1, or records it. `build.md` and the `rust-toolchain-adoption` spec name it as an explicit exception, alongside the nightly lanes, to "every CI toolchain step pins the shipped version". A `cargo check` step is redundant after Clippy and is omitted.

The `rust-toolchain-adoption` requirement "Rust floor is aligned across all stable toolchain surfaces" and its scenario "Stable version surfaces agree" say that **every** CI stable-toolchain reference names the pin. An `@stable` workflow contradicts that as written. So the delta **MODIFIES** that requirement and keeps its three scenario names. It adds one sentence for the advisory-lane exception and qualifies "Stable version surfaces agree" so that the lane alone may name `stable`, explicitly, as an advisory lane. A separate ADDED exception would leave two contradictory requirements.

### D5. Layout-v2 tooling (A4, A3)

- `CARGO_NEXTEST_VERSION` 0.9.137 → **0.9.151** in **both** workflows that pin it, `ci.yml` and `mutation-testing.yml`. `rust-linting-policy` requires every workflow that installs nextest to use the same version. 0.9.151 is ≥ 0.9.143, which brings the layout-v2 dylib-path and artifact-dir fixes, and it is the newest release older than the 7-day cooldown (the same policy as D6). `build.md` states the floor.
- **The local install is not upgraded in this change.** `~/.cargo/bin/cargo-nextest` (0.9.133) is one host-wide binary. The parallel programme runs are testing with it now, and whether `cargo install` replaces it by rename was not verified. Local builds use the pinned cargo 1.98.0, which still writes layout v1, so 0.9.133 keeps working. The upgrade, `cargo install --locked cargo-nextest --version 0.9.151`, is recorded in the programme record as an end-of-programme step for the maintainer.
- `scripts/lint-advisory.py` `GENERATED_OUT_DIR` becomes `/build/[^/]+(?:-|/)[0-9a-f]{16}/out/`. A small self-check (a doctest-like assertion block run by `make lint-advisory`, or the script's existing test entry point if it has one) pins both forms.
- `workspace-hack/Cargo.toml` gets an empty `[lints]` table outside the `### BEGIN HAKARI SECTION` markers. `cargo hakari generate` and `cargo hakari verify` must leave it in place, and that is verified, not assumed. Cargo 1.98 accepts an empty table silently, and cargo 1.100 then stops printing `missing_lints_inheritance`.

### D6. Cooldown refresh and its exceptions (A6 + sourceview5)

`make update-deps-cooldown` runs, in order: (1) `CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="7 days" CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +1.100.0 update`; (2) `cargo hakari generate`; (3) `make cargo-sources`; (4) `cargo +1.98.0 check --workspace --all-targets --all-features --locked`; (5) the whole dependency policy, `cargo deny check advisories bans sources licenses`, not bans alone. `rust-linting-policy` makes all four the normal gate, and a refresh is where a new licence, advisory or source first appears. It fails clearly when the 1.100.0 toolchain is missing.

**Order of the first application.** The target must complete once, end to end, in this change, so its first run is not also its proof of failure. So: (a) add the two measured `deny.toml` skips first; (b) run the whole `make update-deps-cooldown` on the **old** `sourceview5 = "0.11"` floor, which must exit 0; (c) only then raise the floor and apply the `--precise` exception below, followed by hakari, cargo-sources, the 1.98 check and the full deny check again; (d) finally, run the target once more and record its expected "too new" failure.

**New licences, advisories and sources.** At the spec review, advisories, licences and sources passed on the refreshed graph (Measured baseline). A later refresh that fails one of them stops; it is never waved through:

- **Licence.** A new licence is added to `[licenses] allow` only if it is compatible with distributing a GPL-3.0-or-later application. The change that adds it names the crate that brought it and records the reason in a `deny.toml` comment. Otherwise that crate is held with `--precise` and the hold recorded. `zlib-rs` (`Zlib`) is in `Cargo.lock` but outside cargo-deny's graph today, so it needs no entry. `cargo-sources.json` still vendors it, which is accepted: `Zlib` is GPL-compatible, and a vendored but unbuilt crate is not distributed in the binary.
- **Advisory.** It is fixed by a newer release or a `--precise` hold. An `[advisories] ignore` entry needs a reason and a removal condition.
- **Source.** Every source stays crates.io. The cooldown lives in environment variables, never in `.cargo/config.toml`, because cargo 1.98 warns on the unknown key on every command. `cargo install` ignores the cooldown, which is recorded.

**Duplicates.** Both measured duplicates are upstream-structural. `syn` 3 enters through clap_derive ≥ 4.6.4 while tracing-attributes and zerocopy-derive stay on 2. `system-deps` 9 enters through gtk-rs-core 0.22.9 while libadwaita-sys 0.9.2 and sourceview5-sys 0.11.3 stay on 7. A `--precise` hold (clap 4.6.3, or gtk-rs-core 0.22.8) would be undone by the next cooldown run and would keep older upstream fixes out. Decision: version-scoped `skip` entries in `deny.toml`, `{ crate = "syn@2", reason = … }` and `{ crate = "system-deps@7", reason = … }`. Cargo-deny 0.19.5 accepted that form and passed `bans` with it at the spec review; the CI pin 0.19.8 is checked in CI at the end of the programme. Each entry has a reason and a removal condition ("remove when tracing-attributes and zerocopy-derive use syn 3"; "remove when libadwaita-sys and sourceview5-sys use system-deps 9"). If the applied refresh shows other duplicates, each gets the same treatment, or a hold with its reason. The build-time cost of a second `syn` is a few CPU-seconds per clean build and is accepted.

**sourceview5 exception.** The safe binding returns a count only from 0.11.3, so the workspace floor becomes `"0.11.3"`. That is a manifest-range change, which `dependency-surface-maintenance` requires the proposal to list (it does). The order is: run the cooldown refresh with the **old** floor (it resolves 0.11.2), raise the floor, then run `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +1.100.0 update -p sourceview5 -p sourceview5-sys --precise 0.11.3` **without** the cooldown, then hakari, cargo-sources, the 1.98 check and deny. The exception, its reason (it retires an `unsafe` FFI block and an upstream panic LushText reported) and its expiry (2026-11-21 08:57 UTC) are recorded in `build.md`'s cooldown section. Until it expires, `make update-deps-cooldown` fails at step 1 with Cargo's "too new" message. That is accepted and documented: Cargo has no per-package override, and `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow` would disable the cooldown for every crate (measured). The 7-day window was chosen in the investigation and is kept.

**Side lockfiles.**

- **`fuzz/Cargo.lock` must change in this change.** The fuzz crate depends on `lushtext-core`, whose `sourceview5` requirement comes from the root workspace table. After the floor moves to `^0.11.3`, its lock (which holds 0.11.0) no longer satisfies its manifest. It gets step 1's cooldown refresh in `fuzz/` and then the same `--precise 0.11.3` exception. Consistency is proved with no nightly build: `cargo +1.100.0 metadata --locked --format-version 1 --manifest-path fuzz/Cargo.toml` must succeed.
- **`fixtures/gtk-lush-adoption/stock-settle/Cargo.lock`** has no sourceview5. It is refreshed with step 1's command, or its deferral is recorded with the reason.

`make fuzz-corpus-replay` runs in the main workspace, so it runs whatever happens to the fuzz lock. `formal/evaluation/stateright/Cargo.lock` is disposable evaluation tooling outside the spec's side-lockfile list and is left alone, which is recorded.

**Behaviour preservation.** The refresh moves gtk4 0.11.3 → 0.11.5, glib 0.22.7 → 0.22.10, regex 1.12 → 1.13, ignore 0.4.26 → 0.4.34, notify 8.2.0 → 8.2.1, proptest 1.11 → 1.12 (rand 0.10), serde_json and others. It must not change editor, search, Markdown, watching, persistence or automation behaviour. The proof is:

- the full `make test` (unit, integration, every widget shard);
- the property tests;
- `make fuzz-corpus-replay`;
- `make gtk-axioms`;
- `make gtk-lush-msrv`;
- the Kani codegen of `gtk-lush-widgets` (D3);
- the Flatpak build.

`build.md`'s "Toolkit updates" procedure is **not** triggered. It fires on a change of the GTK, Libadwaita, GtkSourceView, Pango or GLib C libraries: the CI image, the Flatpak runtime, the GNOME 50 floor, or the Snap platform. This refresh moves only the Rust bindings. `make gtk-axioms` still runs, as behaviour-preservation evidence, because every probe and sample reaches the toolkit through the refreshed bindings. The ledger's "Verified against" columns are not updated, and the binding versions are recorded in the evidence instead.

### D7. In-tab Replace All uses the safe binding

`replace_all_occurrences` collapses to `context.replace_all(replace)` (or the helper is inlined). Its rustdoc now says the 0.11.3 binding returns the count and treats a zero count without an error as success. The binding passes `replace.len() as i32` where the old code clamped (verified in the 0.11.3 source). `replace_entry` is a `GtkEntry` (`resources/ui/search-bar.blp`). Its `GtkEntryBuffer` holds at most 65,535 characters, which is at most 262,140 bytes of UTF-8, and `entry_paste_cap` limits each insertion to 16 KiB on top of that. So the length cannot overflow `i32`, and the comment says so.

The `Err` arm stays: it logs the `GError` at error level and never unwraps, and `report_match_state` runs in every case. The clause does **not** promise that the log is free of user text. GLib's replacement-parse messages quote the replacement template the user typed. That is not document text, and the diagnostics requirement (`search-replace-safety`, "Search diagnostics never contain private document text") governs it unchanged; the code before this change logged the same message.

The proof:

- The existing widget test `test_replace_all_with_no_match_is_a_no_op` (`background_tab_layout.rs`) is the regression proof for the panic the binding used to raise. It gains an assertion that the counter reports zero matches after the call, which the scenario requires and the test does not check today.
- The two replace-all tests beside it prove the count path.
- One new widget test drives the `Err` arm if GtkSourceView returns a `GError` for a malformed regex replacement (for example a trailing `\` with regular expressions on). It asserts that the document is unchanged, nothing panics, and the counter refreshes. If GtkSourceView 5.18 accepts every template, so the arm has no driver, that is recorded in the evidence and the arm is proved by the binding's type alone. **No new GTK axiom** is needed: the behaviour relied on (a zero count without a `GError` is not a failure) is unchanged from the FFI code, the binding's own test pins it, and the widget test drives the real GtkSourceView.

## Risks / Trade-offs

- **Lockfile churn (129 packages)** could regress behaviour in a binding. Mitigation: the full suite, `make gtk-axioms`, and the Flatpak build on 1.98.0. A regression found is fixed in this change or the offending crate is held with `--precise` and recorded.
- **Merge pressure on parallel runs.** The Clippy sweep touches about 30 files. The change lands first, and the others merge `main`. Generated files (`Cargo.lock`, `cargo-sources.json`, `workspace-hack`) are regenerated, never hand-merged.
- **The cooldown target fails until 2026-11-21.** This is documented in `build.md` with the date. Nothing in CI runs it.
- **Clippy 1.100 sites outside `lushtext-core` are unmeasured.** D3 measures them first. If a site needs a post-1.98 API, it gets an `#[expect]` with a reason, not an MSRV raise.
- **Flaky widget tests owned by PR #99** (`test_autosave_during_a_local_history_restore_keeps_the_previous_draft`, the sidebar directory-retention test): if they flake, this change merges `origin/main` after #99 lands, rather than fixing them itself (coordinator instruction).

## Spec-review decisions (2026-11-14)

The stage-2 review checked the artifacts against the code, the rules, `scripts/kani-shards.py` and `scripts/check-workflow-boundaries.py`. It made these decisions:

1. **Kani compile of `gtk-lush-widgets` (D3).** The D1 shard compiles only `lushtext-core`. A `--only-codegen` run of `gtk-lush-widgets` is added, because the D6 refresh moves its dependencies. Its one regex-found Clippy site is in `examples/`, so A1 is not the reason. Whether any shard reruns is decided from task 3.1's workspace-wide result, for both Kani packages.
2. **The cooldown target completes once (D6).** The skips come first, then a full run on the old floor that must pass, then the floor and the exception, then the expected-failure run.
3. **The whole deny policy (D6).** The target's last step and the spec requirement run all four checks, not bans alone. A licence, advisory or source rule is added. Advisories, licences and sources were measured passing on the refreshed graph.
4. **The GError clause (search-replace-safety).** "Logged without document text" is replaced by what the code does and a test can check: the `GError` is logged, nothing panics, and the counter refreshes. The diagnostics requirement keeps governing message content. The no-match test gains its missing counter assertion, and a driven `Err` test is added if GtkSourceView offers a driver.
5. **The local nextest upgrade (D5)** is deferred to the end of the programme, for the maintainer: it replaces a host-wide binary that parallel runs are using, and nothing local needs it under cargo 1.98. Also, `mutation-testing.yml` was missing from the CI pin bump.
6. **Claims verified or corrected.**
   - D7's `i32` bound holds: `replace_entry` is a `GtkEntry`, and the 0.11.3 source was read.
   - The GTK toolkit-update procedure is triggered only by C-library changes, so D6's "binding bump is a toolkit update" was wrong. `make gtk-axioms` still runs, as evidence.
   - LushText uses neither `match_style` nor `FileExt::location` (grep).
7. **The toolchain alignment conflict (rust-toolchain-adoption).** The "Rust floor is aligned across all stable toolchain surfaces" requirement is MODIFIED, with all three scenario names kept, to name the advisory lane as the one stable exception.
8. **The test-seam ratchet (D2, found by this review).** The ratchet stands at 171 of 171 and counts `#[cfg(test)]` declarations. The design's new `fail_next_parent_sync_with_for_test` would have failed `make check-workflow-boundaries`, so the existing seam takes the error kind instead. No widget-level seam is added, and the sidebar scenarios are proved by composition (D2).
9. **`fuzz/Cargo.lock` must change (D6, found by this review).** The floor raise makes it inconsistent with its manifest, so it is refreshed and checked with `cargo metadata --locked`.
