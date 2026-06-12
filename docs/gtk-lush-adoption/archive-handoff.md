# Archive Handoff

This phase is the maintained internal-platform baseline evidence. A future
publication or graduation proposal may cite it, but must first refresh any
stale evidence and record maintainer approval for that track.

Baseline evidence:

- maintained adoption lab: `crates/gtk-lush-adoption-lab`
- adoption matrix: `docs/gtk-lush-adoption/matrix.toml`
- timed stock `gtk-lush-settle` fixture and journal
- unrelated-project spike notes
- API review decisions
- local adoption checks:
  `make gtk-lush-adoption-matrix`,
  `make gtk-lush-adoption-lab`,
  `make gtk-lush-stock-fixtures`
- headless adoption widget proof:
  `cargo test -p lushtext --test widget gtk_lush_adoption`

Publication-specific work remains dormant future-track work:

- no functional crates were published
- no `0.1.0` release was prepared
- no repository split was performed
- LushText still uses workspace path dependencies
- docs.rs metadata, crates.io credentials, release tags, changelogs, and
  semver baselines remain the responsibility of a later approved publication
  or graduation change
