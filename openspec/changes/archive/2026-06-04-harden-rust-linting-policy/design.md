## Context

LushText currently centralizes many Clippy and rustc lints in the workspace manifest, and the real crates opt into that policy with `[lints] workspace = true`. Local and CI validation run `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`; CI also runs rustdoc linting and `cargo deny check advisories bans sources`. This is a solid baseline, but the exploration found several gaps:

- The normal Clippy gate does not pass `--all-features`, even though the all-feature variant passes cleanly today.
- Broad advisory lint groups surface useful findings, but also large noisy categories: `doc_markdown`, `missing_const_for_fn`, `too_many_lines`, generated widget registry findings, and broad `restriction` philosophy lints.
- High-signal individual lints currently reveal actionable cleanup: `redundant_clone`, `significant_drop_tightening`, `needless_collect`, `unchecked_time_subtraction`, `case_sensitive_file_extension_comparisons`, and `manual_midpoint`.
- Additional design-smell lints reveal smaller but useful cleanup clusters: `derive_partial_eq_without_eq`, `wildcard_imports`, boolean-parameter/boolean-struct excess, `implicit_hasher`, `multiple_crate_versions`, and `cognitive_complexity`.
- rustc advisory probes for `unreachable_pub`, `unused_qualifications`, and `unused_crate_dependencies` produce current findings rather than clean gates. Several are entangled with `workspace-hack`, cargo-hakari, and dev/bench dependencies.
- `cargo deny check licenses` fails because no license allow-list exists yet and the generated `workspace-hack` package lacks license metadata.
- CI installs some validation tooling from moving sources, notably unpinned `cargo-deny` and `cargo-nextest` installers.
- The filesystem-boundary audit is a strong path-aware policy tool; Clippy's `clippy.toml` can enforce globally disallowed methods/types, but it cannot express "disallow outside this module" cleanly.

Current documentation confirms the implementation direction: Cargo workspace lints require member opt-in; Clippy supports `clippy.toml` for configured disallowed methods/types and warns against blanket `clippy::restriction`; cargo-deny supports graph, advisory, ban, source, license, and workspace-dependency policy.

## Goals / Non-Goals

**Goals:**
- Make the default Rust lint gate cover all workspace targets and all enabled workspace features.
- Clean all currently identified high-value lint findings and promote the strongest lints into the blocking workspace lint policy.
- Introduce a repeatable advisory lint lane that discovers and summarizes broad lint groups without making noisy groups hard gates.
- Make cargo-deny a complete dependency policy gate, including licenses and justified duplicate-version handling.
- Pin Rust validation tools installed in CI so lint outcomes do not drift unexpectedly.
- Keep filesystem-boundary enforcement path-aware and deterministic.
- Update contributor, agent, and CI guidance so every documented validation path names the same policy.
- Finish with no unclassified lint/dependency-policy findings from the explored categories.

**Non-Goals:**
- Do not enable `clippy::restriction`, `clippy::pedantic`, `clippy::nursery`, or `clippy::cargo` as blanket blocking groups.
- Do not convert noisy advisory categories such as `doc_markdown`, `missing_const_for_fn`, or `too_many_lines` into hard gates without a separate cleanup and policy decision.
- Do not replace rustdoc, filesystem-boundary, mutation, fuzz, widget, or end-user smoke gates with Clippy.
- Do not introduce nightly-only Cargo lints into stable CI as a blocking requirement.
- Do not remove cargo-hakari or the generated `workspace-hack` pattern solely to satisfy dependency lints.

## Decisions

### Decision: Treat Clippy's blocking policy as curated, not group-based

The implementation will add individual lints only after the current tree is clean for those lints. The first blocking promotion set should include:

- `clippy::manual_midpoint`
- `clippy::unchecked_time_subtraction`
- `clippy::case_sensitive_file_extension_comparisons`
- `clippy::significant_drop_tightening`
- `clippy::needless_collect`
- `clippy::redundant_clone`
- `clippy::derive_partial_eq_without_eq`
- `clippy::wildcard_imports`

Each lint must be evaluated against GTK, generated, test, and benchmark code before being denied. Any remaining exception must use `#[expect(..., reason = "...")]`, not broad `#[allow]`.

Alternative considered: enable `clippy::pedantic` or `clippy::nursery` wholesale. Rejected because the probe produced hundreds of stylistic/doc/const findings that would turn lint hardening into a broad taste rewrite instead of a high-signal quality gate.

Alternative considered: enable `clippy::restriction` wholesale. Rejected because Clippy explicitly discourages blanket restriction lints and the probe produced thousands of incompatible or project-inappropriate warnings.

### Decision: Add `--all-features` to the standard Clippy gate

The normal local, hook, and CI Clippy command will become:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The all-feature version passed during exploration and better covers feature-gated test utilities, property-test code, and fuzz replay support without requiring nightly or cargo-fuzz.

Alternative considered: keep all-feature Clippy separate. Rejected because it is already clean and cheap enough for the main lint job.

### Decision: Add an advisory lint command with explicit categorization

The implementation will add a Makefile/script target such as `make lint-advisory` that runs a documented set of broad and exploratory lint probes, summarizes them, and compares them to an expected policy file or report. The command should include:

- Clippy advisory groups: `pedantic`, `nursery`, `cargo`
- Maintainability probes: `cognitive_complexity`, `too_many_lines`, `too_many_arguments`, `type_complexity`
- Selected design-smell probes: boolean excess, implicit hasher, multiple crate versions, print stdout/stderr, panic/expect/indexing categories
- rustc advisory probes through `RUSTFLAGS` where stable and useful: future compatibility, Edition 2024 compatibility, unreachable public items, unused qualifications, unused crate dependencies, and unsafe-code inventory

The advisory lane must not fail merely because known advisory categories exist. It must fail when output is unparseable, when a new lint category appears without classification, or when a category marked "must stay zero" becomes nonzero.

Alternative considered: no advisory lane, only blocking lints. Rejected because the exploration found useful non-blocking signal that should remain discoverable and repeatable.

### Decision: Keep path-sensitive filesystem rules in the audit script

`clippy.toml` will be used only for globally safe disallowed methods or types. It will not be used to disallow `std::fs`, `rustix`, raw backend APIs, or filesystem engine APIs where LushText already needs path-aware exceptions for private backend modules, build-support code, fixtures, and approved read-only engines. The existing `scripts/check-filesystem-boundary.sh` remains the authoritative path-sensitive filesystem policy gate and should be included in the normal lint/policy validation path.

Alternative considered: use `clippy.toml` `disallowed-methods` for all raw filesystem APIs. Rejected because it would require many local suppressions in approved modules and would be less clear than the current path-aware script.

### Decision: Turn cargo-deny into a complete policy gate

The dependency policy will include licenses in addition to advisories, bans, and sources. The implementation will:

- Add a license allow-list compatible with GPL-3.0-or-later distribution and the current Rust/GTK dependency graph.
- Add license metadata for `workspace-hack` or otherwise configure cargo-deny so the generated package is intentionally licensed.
- Move duplicate-version handling toward deny-by-default with explicit `skip` or `skip-tree` entries for known foundational transitive duplicates such as `windows-sys` and `bitflags` when they cannot be unified safely.
- Revisit `[bans.workspace-dependencies]` settings so unused and duplicate workspace dependencies are either enforced or explicitly justified around cargo-hakari.
- Run `cargo deny check advisories bans sources licenses` in CI.

Alternative considered: leave licenses out because current checks pass. Rejected because the license failure is policy absence, not evidence that the dependency graph is unreviewable.

### Decision: Pin validation tools installed in CI

The implementation will pin versions for Rust validation helpers installed during workflows. At minimum this includes cargo-deny and cargo-nextest; cargo-fuzz and any other Rust validation helper installed by CI should also be pinned or documented as an explicit nightly/toolchain exception. Pinning should live in one obvious place when practical, such as workflow env variables or reusable Makefile variables.

Alternative considered: rely on `--locked` without a version. Rejected because `--locked` makes installation reproducible for the selected version, but does not prevent the selected version from changing when the command uses the latest published crate.

### Decision: Clean rustc lint candidates before hardening them

`unused_qualifications` and `unreachable_pub` are worthwhile cleanup targets, but they currently produce many findings. The implementation will clean them where they improve clarity, then decide whether each is suitable as a blocking lint, a must-stay-zero advisory category, or a documented accepted advisory category. `unused_crate_dependencies` must be handled carefully because cargo-hakari, `workspace-hack`, dev dependencies, benchmarks, and generated crates create legitimate noise.

Alternative considered: add these rustc lints immediately. Rejected because the first run would fail and the raw output would be hard to interpret without policy around generated and hakari-managed crates.

## Risks / Trade-offs

- Stricter lints may create churn in GTK adapter code -> Mitigate by promoting only lints that produce clear value and allowing narrow `#[expect]` exceptions with reasons.
- Advisory lint reports can become ignored noise -> Mitigate by requiring every lint category to be classified and by failing on new unclassified categories.
- License policy may reveal dependencies with uncommon or missing metadata -> Mitigate by using cargo-deny's allow-list and exception mechanisms with explicit rationale, not silent ignores.
- Duplicate-version enforcement can fight the GTK/transitive dependency graph -> Mitigate with deny-by-default plus narrow `skip` or `skip-tree` entries for proven foundational duplicates.
- Tool pinning can require periodic maintenance -> Mitigate by centralizing versions and documenting the update process.
- Adding filesystem-boundary audit to default linting can catch pre-existing drift -> Mitigate by fixing the drift in the same work stream rather than bypassing the audit.

## Migration Plan

1. Establish the stricter commands and advisory report shape in scripts/Makefile without changing source behavior.
2. Clean current high-signal Clippy findings and promote those lints to `[workspace.lints.clippy]`.
3. Clean or classify rustc advisory findings.
4. Add the cargo-deny license and duplicate-version policy, then update CI to run the complete policy.
5. Pin CI-installed validation tools and verify workflow syntax.
6. Update guidance in README, AGENTS, `.agents/rules`, and relevant nested guidance.
7. Run the full validation ladder and close every task with evidence.

Rollback is configuration-only for most changes: revert newly added lint denies, advisory policy files, deny.toml changes, and workflow pin changes. Source cleanup should remain valid even if a particular lint promotion is rolled back.

## Open Questions

- Which exact license allow-list entries are required after running a full cargo-deny license pass on the final dependency graph?
- Which duplicate crate versions can be unified through dependency updates, and which require explicit cargo-deny exceptions?
- Is `redundant_clone` clean enough to deny globally after GTK object ownership and signal-closure patterns are reviewed, or should it become a must-stay-zero advisory for production modules only?
- Should the advisory lint report be committed as a baseline file, generated into `docs/`, or kept as a script output compared against a compact allow-list?
