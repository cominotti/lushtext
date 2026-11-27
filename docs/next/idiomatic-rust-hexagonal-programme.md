# Idiomatic Rust & Hexagonal Architecture Programme

This document tracks the five-change programme refining LushText's crate architecture, domain primitive boundaries, service error contracts, widget state machines, and cold-path ergonomics without regressing formal verification guarantees, responsiveness budgets, or packaging invariants.

## Programme Scope & Sequencing

Changes land strictly sequentially (`1 -> 2 -> 3 -> 4 -> 5`), each shipping its own failing-first policy gate and full CI validation before the next change starts from updated `origin/main`:

1. **`complete-hexagonal-crate-organization`** — Rename `crates/lushtext-core` -> `crates/lushtext-ui`, organize `lushtext-model` and `lushtext-services` into bounded contexts, eliminate remaining `regex` / `pulldown-cmark` infra leaks from `lushtext-ui` and `lushtext-model`, relocate GTK-free tests and Criterion benches out of `lushtext-ui`, and add `scripts/check-hexagonal-boundaries.py`.
2. **`domain-value-types-and-primitive-boundaries`** — Introduce zero-cost layout-verified value types (`lushtext_model::values::{Bytes, Sp, Micros, CharCount, LineIndex}`, typed generations, typed domain IDs), eliminate `>= 2` positional `bool` parameter lists (`fn_params_excessive_bools = "deny"`), extend `scripts/check-threshold-kinds.py`, and add `scripts/check-domain-value-types.py`.
3. **`typed-service-error-boundaries`** — Standardize on domain-named `thiserror` error enums across `lushtext-services` and UI/conformance boundaries, remove `anyhow` from production `lushtext-services` and `lushtext-ui`, retire `Result<_, String>` service signatures, resolve the rule conflict in `.agents/rules/rust.md`, and add `scripts/check-error-contracts.py`.
4. **`idiomatic-rust-state-machines-and-c-ism-removal`** — Convert flat parallel `bool`/`Option` workflow state bags into algebraic phase enums inside bounded coordination modules, slim oversized `imp.rs` files to `<= 500` lines, consolidate multi-slot `Box<dyn Fn...>` callback farms, eliminate C-isms (`reset_*_state`, raw `-1` sentinels in policy, `while idx < len`, unsafe `as` casts), enable `struct_excessive_bools` / `cast_possible_wrap`, and add `scripts/check-idiomatic-state.py`.
5. **`cold-path-ergonomics-and-shared-metadata`** — Codify the Two-Zone Rust Rule (zero-cost hot/Kani/arena vs. readable cold UI/orchestration), introduce `SharedPath`/`SharedStr` (replacing `Arc<PathBuf>` and hot `.clone()` churn), move `install_execution.rs` note bodies onto `Arc<DisposalOwned<String>>`, box cold optional state, reduce cold `<F: Fn*>` monomorphization, and add `scripts/check-arc-safety.py`.

## Baseline Metrics (`main` at `3c442112` / `ddfb7e2b`)

| Dimension | Baseline Count | Target Gate | Change |
| --- | --- | --- | --- |
| `crates/lushtext-core` crate name | 1 crate (`lushtext-core`) | `lushtext-ui` + `scripts/check-hexagonal-boundaries.py` | Change 1 |
| Top-level `src/` items (`model` / `services`) | 53 / 44 | Bounded-context subtrees (`check-hexagonal-boundaries.py`) | Change 1 |
| Direct infra leaks (`regex` in `model`/`ui`, `pulldown-cmark` in `ui`) | 3 sites (`content_search.rs`, `protected_view/find_execution.rs`, `search_panel/policy.rs`) + 8 `markdown_preview` files | 0 leaks (`check-hexagonal-boundaries.py`) | Change 1 |
| Production files `>= 1,000` prod lines (excluding `#[cfg(test)]` & test files) | **39** legacy files on `main` (9 masked on pre-fix substring check) | `<= 1,000` lines for all new files; descending ceiling `<= 39` | Change 1 |
| Raw `*generation*` fields / `editor_id: usize` | 204 / 5 | 0 at boundary seams/policies/states (`check-domain-value-types.py`) | Change 2 |
| Functions with `>= 2` positional `bool` params | 48 app + 1 axiom helper (`a18.rs`) | 0 (`clippy::fn_params_excessive_bools = "deny"`) | Change 2 |
| `anyhow` files (`services` / `ui`) & `Result<_, String>` | 22 / 12 & 19 | 0 (`check-error-contracts.py` + dep ban) | Change 3 |
| Flat state bags / `imp.rs > 500L` / `imp.rs Box<dyn Fn>` | 35 / 9 / 49 (across 11 `imp.rs`) | 0 bags / 0 oversized `imp.rs` / `<= 2` slots (`check-idiomatic-state.py`) | Change 4 |
| `reset_*_state` / `-1` policy sentinels / `while idx < len` / `as` casts | 5 / 2 / 25 / 323 | 0 `reset_*_state`, 0 policy `-1`, idiomatic iterators & checked casts | Change 4 |
| Naked `Arc<String>` (`install_execution.rs`) / `Arc<PathBuf>` (`palette/index/`) / cold `<F: Fn*>` | 6 / 14+ / 85 | 0 naked `Arc<String>`, 0 `Arc<PathBuf>`, `<= 25` cold `<F: Fn*>` (`check-arc-safety.py`) | Change 5 |

## Production File Size Ratchet Policy

`scripts/check-hexagonal-boundaries.py` counts production lines per `.rs` file in `crates/lushtext-model/src`, `crates/lushtext-services/src`, and `crates/lushtext-ui/src`, stripping every `#[cfg(test)]` item/module and excluding dedicated test modules (`*/tests.rs`, `*_tests.rs`, `*/tests/**`, `*/kani_proofs.rs`, `*/kani_proofs/**`, `*/conformance/**`, `*/fixture.rs`). Any file outside the recorded legacy ceiling exceeding 1,000 production lines fails `make check-policy`; shrinking or splitting a legacy file below 1,000 lines lowers the ceiling in subsequent changes.

## Completion Outcomes

*Updated at the close of each change.*

| Change | PR / Merge SHA | Archive Path | Outcome Summary |
| --- | --- | --- | --- |
| 1. `complete-hexagonal-crate-organization` | pending (branch `refactor/complete-hexagonal-crate-organization`) | pending | Renamed `crates/lushtext-core` to `crates/lushtext-ui`; retired legacy re-export shims; organized `lushtext-model` into 6 bounded-context subtrees (`values/`, `document/`, `workspace/`, `persistence/`, `budgets/`, `automation/`) and `lushtext-services` into driven adapters (`filesystem/`, `persistence/`, `watch/`) + 7 application-service modules; encapsulated `regex` (`services::search::regex_query`, `preview`) and `pulldown-cmark` (`services::notes::markdown_render`) out of `lushtext-model`/`lushtext-ui`; relocated `persistent_json_format`, `text_scan_allocations`, 10 model + 16 service property modules, and 28 GTK-free Criterion benchmark groups (3 model + 25 service) out of `lushtext-ui`; enforced `scripts/check-hexagonal-boundaries.py` (`0` crate leaks, legacy `> 1,000`-line ratchet `37 <= 39`). |
| 2. `domain-value-types-and-primitive-boundaries` | pending | pending | pending |
| 3. `typed-service-error-boundaries` | pending | pending | pending |
| 4. `idiomatic-rust-state-machines-and-c-ism-removal` | pending | pending | pending |
| 5. `cold-path-ergonomics-and-shared-metadata` | pending | pending | pending |
