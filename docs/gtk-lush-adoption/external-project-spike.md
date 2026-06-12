# Unrelated Existing Project Spike

Date: 2026-06-12

Selected GTK Lush crate: `gtk-lush-settle`

## Candidate 1: `gtk-rs/gtk4-rs`

- Repository: `https://github.com/gtk-rs/gtk4-rs`
- Commit: `a27955b0ef0215d5a93529c16d733e176a4419ba`
- License: MIT in the root workspace metadata and license files; compatible
  for a local uncommitted spike and bounded notes.
- Rationale: canonical gtk-rs GTK4 project with an `examples` package.

Patch shape:

- add one `gtk-lush-settle` path dependency to `examples/Cargo.toml`
- add one tiny `gtk_lush_settle_probe` binary using `Debounce`,
  `SettleBurst`, and `SupersedingTimer`

Command:

```sh
cargo check -p gtk4-rs-examples --bin gtk_lush_settle_probe
cargo check -p gtk4-rs-examples --bin gtk_lush_settle_probe --features v4_10
```

Result:

- The default check failed inside the upstream `gtk4` crate because the current
  checkout imports accessibility APIs gated behind disabled version features.
- The `v4_10` rerun still failed inside upstream generated GTK bindings because
  the checkout needs broader feature/version alignment than the examples
  package exposed in this environment.
- No GTK Lush API error was reached before upstream build friction.

Classification:

- Feature flag: upstream checkout feature selection, not GTK Lush.
- Accepted limitation: do not treat this candidate as the main adoption signal.

## Candidate 2: `SeaDve/Kooha`

- Repository: `https://github.com/SeaDve/Kooha`
- Commit: `b5c5c3aa1122b84d477617de73b0fdb21b1a27b2`
- License: GPL-3.0-or-later (`COPYING`), compatible with LushText's
  GPL-3.0-or-later license for a local uncommitted spike and bounded notes.
- Rationale: real GTK4/Libadwaita application using the same gtk4/libadwaita
  generation as LushText.

Temporary checkout isolation:

- The ignored checkout belongs under
  `build/gtk-lush-adoption/external-checkouts/`.
- Because the ignored checkout lives under the LushText repository root, the
  local spike added a temporary `[workspace]` table to Kooha's manifest so Cargo
  would not try to enroll it in LushText's workspace.

Patch shape:

- add `gtk-lush-settle = { path = "../../../../crates/gtk-lush/settle" }`
- add `src/bin/gtk_lush_settle_probe.rs` using only `Debounce`,
  `SettleBurst`, and `SupersedingTimer`

Command:

```sh
cargo check --bin gtk_lush_settle_probe
```

Result:

- Cargo resolved `gtk-lush-settle v0.0.0` as the only GTK Lush package.
- The build stopped on missing Kooha host dependencies:
  `gstreamer-1.0`, `gstreamer-base-1.0`, `gstreamer-video-1.0`,
  `gstreamer-allocators-1.0`, and `gstreamer-gl-1.0` pkg-config files.
- No GTK Lush compile, version, naming, feature, or dependency-direction error
  appeared before the host dependency wall.

Classification:

- Documentation: note that real external apps may require their own native
  dependency setup before a GTK Lush probe can finish.
- Feature flag: no GTK Lush issue.
- Type-shape: no GTK Lush issue.
- Accepted limitation: external project checks should remain bounded notes
  unless the phase explicitly installs third-party app dependency stacks.

## Decision

The external spike found no GTK Lush API hardening item. The only actionable
follow-up is documentation: keep the maintained stock fixture and adoption lab
as deterministic local gates, and treat unrelated-project checkouts as bounded,
ignored evidence unless a future phase chooses a project with a smaller native
dependency footprint.
