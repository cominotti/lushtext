# GTK Lush

GTK Lush is the in-tree staging area for extracting LushText's hardened
GTK4/Libadwaita patterns into small, independently adoptable Rust crates.

The family is governed by [GOVERNANCE.md](GOVERNANCE.md) and the umbrella
vision in `docs/next/gtk-lush.md`. The current family members are functional
in-tree `0.0.0` APIs for LushText and future adoption testing:

- `gtk-lush-signals`
- `gtk-lush-settle`
- `gtk-lush-tasks`
- `gtk-lush-viewport`
- `gtk-lush-widgets`
- `gtk-lush-proof-harness`
- `gtk-lush-proof-spine`

The companion `cargo-gtk-proof` binary is a workspace proof tool under
`crates/cargo-gtk-proof`, not a GTK Lush family crate. It may consume proof
family crates, schemas, and LushText smoke artifacts while leaving the family
crates as independently adoptable leaves.

They are not Phase 5 publication-ready crates. External stability and any
`0.1.0` publication still require the second-consumer, adoption-test, semver,
public-API, and maintainer approval gates in `GOVERNANCE.md`.

Run the family-specific checks with:

```sh
make check-gtk-lush-policy
make gtk-lush-doctests
make gtk-lush-examples
make gtk-lush-msrv
make gtk-lush-api-advisory
```
