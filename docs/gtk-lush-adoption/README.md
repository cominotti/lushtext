# GTK Lush Adoption Evidence

This directory holds the bounded evidence for
`validate-gtk-lush-adoption-surface`. It is the maintained baseline evidence
for GTK Lush's in-tree internal platform. It can support a future
publication/graduation proposal, but it does not require functional `0.1.0`
publication, repository split, or LushText dependency migration by itself.

## Locations

- Maintained adoption lab: `crates/gtk-lush-adoption-lab`
- Stock one-crate fixture: `fixtures/gtk-lush-adoption/stock-settle`
- Adoption matrix: `docs/gtk-lush-adoption/matrix.toml`
- Timed stock journal: `docs/gtk-lush-adoption/timed-stock-settle.md`
- Unrelated-project spike note:
  `docs/gtk-lush-adoption/external-project-spike.md`
- API review: `docs/gtk-lush-adoption/api-review.md`
- Specialist review notes: `docs/gtk-lush-adoption/review-notes.md`
- Archive handoff: `docs/gtk-lush-adoption/archive-handoff.md`

Generated adoption artifacts, large logs, screenshots, and temporary external
checkouts stay out of git under `build/gtk-lush-adoption/`.

## Local Checks

```sh
make gtk-lush-adoption-matrix
make gtk-lush-adoption-lab
make gtk-lush-stock-fixtures
make check-gtk-lush-adoption
```

The stock fixture check uses committed files and path dependencies only. It does
not require crates.io publication, LushText resources, LushText GSettings
schemas, or another GTK Lush family crate.
