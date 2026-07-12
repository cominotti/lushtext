# Meson and Cargo integration

Use this reference when changing the root Meson graph, Cargo wrapper, resources, install rules, profile selection, or vendored dependencies.

## Ownership

- Cargo builds Rust workspace packages.
- Meson owns installed application integration: binary install, resources, schemas, desktop metadata, AppStream metadata, icons, and translations.
- `flatpak-builder` invokes the current Meson graph in an offline sandbox.
- `build-aux/cargo.sh` bridges Meson paths/profile data into Cargo. Inspect its actual argument contract before changing either side.

Do not copy a generic dual-resource strategy or wrapper. Confirm how `crates/lushtext-core/build.rs`, runtime resource registration, `resources/meson.build`, and the installed `pkgdatadir` work together in the current tree.

## Dependency vendoring

Use the repository command:

```bash
make cargo-sources
```

Run it after dependency, feature, source, or lockfile changes. Review and commit the generated
`build-aux/cargo-sources.json`. The active manifest and Cargo environment must keep network access
disabled during the build and must allow Cargo to find the generator-produced source configuration.

## Validation

```bash
make meson-build
make meson-test
make flatpak
make flatpak-install
make verify-flatpak-identity
git diff --check
```

Also run `make check-flatpak-permissions` for any packaging/policy change. A direct Cargo build is
not proof that Meson installs the right resources or that the Flatpak export has the right identity.
Likewise, `make flatpak` is build-only: install the just-built result immediately before identity
verification, or describe the result as build evidence without making claims about installed state.
