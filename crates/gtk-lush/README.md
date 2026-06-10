# GTK Lush

GTK Lush is the in-tree staging area for extracting LushText's hardened
GTK4/Libadwaita patterns into small, independently adoptable Rust crates.

The family is governed by [GOVERNANCE.md](GOVERNANCE.md) and the umbrella
vision in `docs/next/gtk-lush.md`. During the foundation phase, only `0.0.0`
placeholder crates live here:

- `gtk-lush-signals`
- `gtk-lush-settle`

The placeholders intentionally expose no public API. Their purpose is to prove
workspace, policy, documentation, licensing, examples, CI, and crates.io
reservation rails before the dedicated `extract-gtk-lush-signals-and-settle`
follow-up designs real APIs.

Run the family-specific checks with:

```sh
make check-gtk-lush-policy
make gtk-lush-doctests
make gtk-lush-examples
make gtk-lush-msrv
make gtk-lush-api-advisory
```
