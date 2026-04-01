---
description: Build system and compilation rules
globs: "{Cargo.toml,Makefile,.cargo/**,.config/**,build.rs,meson.build}"
---

# Build Rules

## Dev Builds

Use `make` targets for development. The Makefile auto-detects mold and nextest.

```
make run        # build + launch the app
make test       # all tests
make check      # clippy + fmt
```

Direct `cargo` works too, but won't have mold linker unless you export RUSTFLAGS manually.

## Compilation Speed

These patterns are replicated from invowk-rust and must be maintained:

1. **Profiles** in workspace `Cargo.toml` — do not change without benchmarking.
2. **Mold linker** set via Makefile `RUSTFLAGS`, NOT in `.cargo/config.toml` (so builds work without mold).
3. **cargo-hakari** — run `cargo hakari generate` after any dependency change.
4. **.config/nextest.toml** — configure test parallelism here.

## Adding Dependencies

1. Add to `[workspace.dependencies]` in root `Cargo.toml`.
2. Reference with `{ workspace = true }` in crate `Cargo.toml`.
3. Run `cargo hakari generate` to update workspace-hack.
4. Verify gtk-rs version alignment if adding any gtk/glib/gio/pango crate.

## GResources

- `build.rs` in `lushtext-core` compiles resources for dev builds via `glib-build-tools`.
- Resources are embedded in the binary via `include_bytes!` in `lib.rs`.
- For Flatpak, Meson compiles and installs resources separately (planned).

## GSettings Schemas

- Schema XML: `data/dev.cominotti.lushtext.gschema.xml`
- `build.rs` in `lushtext-core` runs `glib-compile-schemas data/` to produce `data/gschemas.compiled` (gitignored).
- `lib.rs::init_schema_dir()` sets `GSETTINGS_SCHEMA_DIR` to point to `data/` for dev builds. Installed builds use the system schema directory.
- Requires `glib-compile-schemas` on the build machine (from `glib2-devel` / `libglib2.0-dev`).
- Widget tests use `GSETTINGS_BACKEND=memory` for isolation (set in `ensure_gtk_init()`).

## Flatpak (Planned)

See `docs/next/flatpak-packaging.md` for the full plan. Key requirements:
- Meson wraps Cargo for install targets.
- `flatpak-cargo-generator` pre-fetches crate sources for offline builds.
- Runtime: `org.gnome.Platform`, SDK extension: `org.freedesktop.Sdk.Extension.rust-stable`.
