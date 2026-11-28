# Delta for visual-geometry-invariants

## ADDED Requirements

### Requirement: Visual proof policy excludes widget tests and preserves digests across commits
Both `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs` SHALL exclude `crates/lushtext/tests/widget/` from visual-sensitive smoke triggers and SHALL compute `changed_files` as the union of `base_ref...HEAD` and working-tree status when a base ref is supplied so digests stay identical across staging and commit.

#### Scenario: Widget test edits do not trigger visual-geometry-smoke
- **WHEN** a change modifies `crates/lushtext/tests/widget/window.rs` alone
- **THEN** both Python and Rust `is_visual_sensitive` predicates return false and self-tests assert parity

#### Scenario: Visual digest survives commit under base-ref
- **WHEN** `visual-geometry-smoke` runs with `--base-ref origin/main` on dirty UI edits and those edits are then committed cleanly
- **THEN** `cargo run -q -p cargo-gtk-proof -- policy --base-ref origin/main` passes against the same summary without re-running Mutter
