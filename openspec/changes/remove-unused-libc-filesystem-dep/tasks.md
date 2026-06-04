## 1. Confirm the leftover

- [x] 1.1 Re-run a workspace-wide search to confirm `libc::`/`use libc` appears in zero `.rs` files under `crates/**/src`, `tests`, `benches`, and build scripts.
- [x] 1.2 Confirm `libc` is declared only in `crates/lushtext-core/Cargo.toml` and the root `[workspace.dependencies]`, and in no other crate manifest.

## 2. Remove the unused dependency

- [x] 2.1 Remove `libc = { workspace = true }` from `crates/lushtext-core/Cargo.toml`.
- [x] 2.2 Remove `libc = "0.2"` from the root `Cargo.toml` `[workspace.dependencies]`.
- [x] 2.3 Update the `services::filesystem::sys` module doc comment so it no longer lists `libc` as a kept backend, matching the rustix-only reality.

## 3. Keep dependency artifacts consistent

- [x] 3.1 Run `cargo hakari generate` and stage any `workspace-hack` changes. (No changes detected — `libc` was not in the hakari unification set.)
- [x] 3.2 Run `make cargo-sources` to regenerate `build-aux/cargo-sources.json`. (No diff — `libc` stays vendored transitively via the GTK stack, so the package set is unchanged.)

## 4. Close the audit gap

- [x] 4.1 Add a deterministic check to `scripts/check-filesystem-boundary.sh` that fails when a controlled raw-backend crate (currently `libc`) is declared in a crate manifest but has no `::`/`use` reference in that declaring crate's source.
- [x] 4.2 Keep the check scoped to a small named allowlist of controlled backend crates so cfg-gated-but-present usage still counts as used and unrelated crates are unaffected.

## 5. Validate and close

- [x] 5.1 Run `./scripts/check-filesystem-boundary.sh` and confirm it passes after removal (and that it would fail if `libc` were re-added unused — verified with a temporary re-add producing exit 1).
- [x] 5.2 Run `cargo fmt --check`.
- [x] 5.3 Run `cargo build`/`cargo check` and `cargo clippy` for the workspace to confirm nothing depended on `libc`. (Cleared 6 pre-existing clippy blockers in the uncommitted boundary work: four `0_u8` separated-suffix lints in `sys.rs` and missing `# Errors` docs on `fixture::{set_xattr, get_xattr}`.)
- [x] 5.4 Run `openspec validate remove-unused-libc-filesystem-dep --strict`.
- [x] 5.5 Mark tasks complete and leave the change apply-ready with implementation evidence captured.
