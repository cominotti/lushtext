## Context

`complete-rustix-filesystem-boundary` moved every Unix backend operation, including Linux ACL/user-xattr preservation, onto `rustix` 1.1.x. The pinned rustix version exposes `listxattr`, `getxattr`, `setxattr`, and `fsetxattr`, so the durable-write metadata-preservation contract no longer needs any direct `libc` call. A workspace-wide search confirms `libc::` appears in zero Rust source files, yet `libc` is still declared in `crates/lushtext-core/Cargo.toml` (via the workspace dependency in the root `Cargo.toml`), and `services::filesystem::sys`'s module doc still lists `libc` as a backend it keeps.

`scripts/check-filesystem-boundary.sh` greps source roots for `libc::` and other raw patterns and passes, because there are no raw libc *calls*. It never inspects `Cargo.toml`, so a declared-but-unused backend crate is invisible to it. The leftover is therefore real but undetectable by the current audit.

## Goals / Non-Goals

**Goals:**

- Remove the unused direct `libc` dependency from `lushtext-core` and the workspace root dependency table.
- Correct the `sys.rs` module doc so it describes the rustix-only backend accurately.
- Keep the cargo-hakari workspace-hack and Flatpak `cargo-sources.json` consistent with the new dependency graph.
- Add a deterministic audit check so a declared-but-unused controlled backend crate fails the boundary audit, preventing this leftover class from recurring.

**Non-Goals:**

- No durable-write contract changes, no metadata-preservation behavior changes, and no runtime behavior changes.
- No change to rustix usage, the public filesystem API, or any caller.
- No general-purpose unused-dependency linter (for example `cargo-machete` or `cargo-udeps`); the check stays narrow to the controlled raw-backend crates the boundary owns, consistent with the existing grep-based script style.

## Decisions

### Decision: Remove `libc` from both the crate and workspace dependency tables

`libc` is declared in `crates/lushtext-core/Cargo.toml` as `{ workspace = true }` and defined once in the root `[workspace.dependencies]`. No other crate references it. Remove both entries so the workspace dependency table does not advertise an unused crate.

Alternative considered: leave the root `[workspace.dependencies]` entry "for future use." Rejected — that is exactly the kind of latent leftover the prior change's no-leftovers goal forbids, and rustix is the chosen backend going forward.

### Decision: Fix the backend module doc rather than leave it aspirational

`sys.rs` claims it keeps `std::fs`, Unix extension traits, `libc`, and `rustix`. After this change it keeps `std::fs`, a single Unix extension import (`OsStringExt`), and `rustix`. Update the comment so the documented exception surface matches reality; a stale comment was explicitly called out as a leftover to remove in the prior change.

### Decision: Extend the existing audit script, not add new tooling

Add a step to `scripts/check-filesystem-boundary.sh` that, for each controlled backend crate (currently just `libc`), fails if the crate's `Cargo.toml` declares it but no `.rs` file under that crate references it. This is a small, deterministic grep/manifest check that matches the script's existing style and stays scoped to backend crates the boundary controls.

Alternative considered: adopt `cargo-machete`/`cargo-udeps`. Rejected — both add tooling/nightly requirements and scan the whole graph, which is broader than needed and heavier than the repo's lean audit posture.

## Risks / Trade-offs

- [Risk] Removing `libc` could break a build if a hidden `#[cfg]`-gated or platform-specific use exists. → Mitigation: workspace-wide `libc::`/`use libc` search returned zero hits across `src`, `tests`, `benches`, and build scripts; confirm with `cargo build`/`cargo clippy` after removal.
- [Risk] Forgetting to regenerate dependency artifacts leaves `workspace-hack`/`cargo-sources.json` stale. → Mitigation: run `cargo hakari generate` and `make cargo-sources` as explicit tasks, per the build rules' dependency-change chain.
- [Risk] The new manifest check could misfire for a controlled crate that is legitimately used only behind a `#[cfg]`. → Mitigation: keep the check scoped to a small named allowlist of controlled backend crates and match any `::`/`use` reference anywhere in the crate's source, so a cfg-gated-but-present use still counts as used.

## Migration Plan

1. Remove `libc` from `crates/lushtext-core/Cargo.toml` and the root `[workspace.dependencies]`.
2. Update the `sys.rs` module doc comment.
3. Run `cargo hakari generate`, then `make cargo-sources`.
4. Add the declared-but-unused backend-dependency check to `scripts/check-filesystem-boundary.sh`.
5. Run the boundary audit, `cargo fmt --check`, `cargo build`/`cargo clippy`, and `openspec validate --strict`.

Rollback is a straight revert of the change artifacts and implementation commit; no persisted data is involved.

## Open Questions

- None. The rustix xattr coverage that made `libc` unnecessary was already confirmed by the prior change.
