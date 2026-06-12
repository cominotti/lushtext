# Archive Handoff

This phase may be cited by `graduate-and-publish-gtk-lush` for:

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

Publication-specific work remains future work:

- no functional crates were published
- no `0.1.0` release was prepared
- no repository split was performed
- LushText still uses workspace path dependencies
- docs.rs metadata, crates.io credentials, release tags, changelogs, and
  semver baselines remain the responsibility of the later publication phase
