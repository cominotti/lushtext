## Context

LushText centralizes most Rust dependencies in the workspace root `Cargo.toml`, ships through an offline Flatpak build that consumes `build-aux/cargo-sources.json`, and also carries side lockfiles for the fuzz crate and the GTK Lush stock adoption fixture. A dependency review on June 23, 2026 found a safe compatible refresh path for the current manifest ranges, plus a few tempting non-compatible or pre-release paths that are better treated as separate changes.

The main workspace is already on the current GNOME 50 binding line: `gtk4 0.11`, `libadwaita 0.9`, `sourceview5 0.11`, `glib/gio 0.22`, and the Flatpak manifest targets GNOME Platform 50. The practical dependency work is therefore lockfile and vendoring maintenance, not a GTK stack migration.

## Goals / Non-Goals

**Goals:**

- Refresh stable, compatible dependency locks for the workspace and side crates.
- Keep Flatpak vendoring metadata exactly aligned with the refreshed workspace lockfile.
- Preserve product behavior for the editor, Markdown preview, search, workspace sidebar, persistence, automation, and packaging identity.
- Make dependency-adoption decisions explicit so future feature work starts from a clear boundary.

**Non-Goals:**

- Do not adopt pre-release `notify-debouncer-full 0.8` or `notify` 9 APIs in this change.
- Do not move `sha2` to the 0.11 major line unless a separate `cargo-gtk-proof` need appears.
- Do not enable additional `pulldown-cmark` parser flags, such as math, wikilinks, metadata blocks, or smart punctuation.
- Do not refactor GTK/Libadwaita surfaces to use newly available widgets or runtime features.
- Do not change runtime permissions, GSettings schemas, app-data formats, automation APIs, or user-visible UI behavior.

## Decisions

### Compatible Refresh Only

Run compatible lock refreshes within the current manifest ranges first. The review identified stable direct updates for `assert_cmd`, `ignore`, `memchr`, `pulldown-cmark`, `regex`, and `serde_json`, alongside transitive maintenance updates and duplicate dependency cleanup.

Alternative considered: widen direct manifest requirements or take latest major/pre-release versions in the same change. Rejected because the known product payoff is low and the validation surface would shift from maintenance to migration.

### Include Side Lockfiles

Refresh `fuzz/Cargo.lock` and `fixtures/gtk-lush-adoption/stock-settle/Cargo.lock` in the same maintenance stream. The fuzz side lock has a small stable update including `libfuzzer-sys 0.4.13`; the stock fixture only needs proc-macro patch refreshes.

Alternative considered: leave side locks untouched. Rejected because it preserves avoidable drift and makes later maintenance harder to reason about.

### Regenerate Flatpak Cargo Sources After the Workspace Lock

After `Cargo.lock` changes, regenerate `build-aux/cargo-sources.json` with the repo target so Flatpak's offline build source list matches the lockfile. This is part of the same atomic change as the lockfile refresh.

Alternative considered: update Cargo locks only. Rejected because Flatpak builds would become stale or fail during sandboxed/offline packaging.

### Defer Feature Adoption to Separate Proposals

Treat platform and parser features as follow-up candidates, not part of this dependency refresh. The deferred candidates are:

- `notify-debouncer-full 0.8.0-rc.2` / `notify` 9 watcher semantics for possible materialized-watch management improvements.
- `pulldown-cmark` flags such as smart punctuation, math, wikilinks, metadata blocks, superscript, and subscript.
- GNOME 50 / Libadwaita 1.9 UI affordances such as sidebar/view-switcher components, reduced-motion audits, and GTK builder diagnostics.
- `sha2 0.11` if `cargo-gtk-proof` later needs a hashing API or backend change.

Alternative considered: fold low-risk parser or UI improvements into the dependency refresh. Rejected because LushText's Markdown preview renders custom GTK event streams, and UI feature adoption requires state-extreme and visual proof coverage beyond dependency maintenance.

## Risks / Trade-offs

- Compatible transitive updates can still change behavior subtly -> mitigate with `make check`, focused content-search/Markdown/fuzz validation, side-fixture checks, and Flatpak vendoring/build validation.
- Flatpak source regeneration can drift if the generator version differs across machines -> mitigate by using the repo `make cargo-sources` target and reviewing `cargo-sources.json` as generated artifact data.
- Lockfile refresh may alter dependency deduplication or feature unification -> mitigate by reviewing `cargo tree --duplicates` and workspace build/test behavior after the refresh.
- Side lock updates may pull local path package metadata into the fuzz lock -> mitigate by reviewing `fuzz/Cargo.lock` for expected path dependency/version changes and replaying the stable fuzz corpus.
- Deferred features may be forgotten -> mitigate by recording the deferral decisions in the implementation notes and leaving separate proposals for feature work.

## Migration Plan

1. Refresh the main workspace lock within current manifest ranges.
2. Refresh the fuzz crate and stock fixture lockfiles.
3. Regenerate Flatpak Cargo sources from the refreshed workspace lock.
4. Review generated diffs for unexpected manifest, feature, permission, or application-code changes.
5. Run validation gates and fix any dependency-induced regressions in the same change.
6. Roll back by reverting the lockfile and generated vendoring changes if validation exposes unacceptable behavior.

## Open Questions

- Should future GNOME 50/Libadwaita feature adoption become one UI-platform proposal or separate proposals per surface?
- Should `notify-debouncer-full` be revisited as soon as 0.8 stabilizes, or only when workspace watcher work is already underway?
