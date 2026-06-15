# snap-packaging Specification

## Purpose
Define LushText's Snap packaging contract for the Meson/Cargo build path,
confined resource loading, GNOME platform dependency gating, and documentation.

## Requirements
### Requirement: Snap Build Reuses the Meson/Cargo Pipeline
The Snap SHALL build the LushText binary through the existing Meson build that
wraps Cargo (`build-aux/cargo.sh`), rather than a separate Snap-only build path,
so the Snap and Flatpak stay aligned on a single build definition.

#### Scenario: snapcraft.yaml drives the Meson build
- **WHEN** the Snap part for LushText is built
- **THEN** it invokes the project `meson.build` (which calls `cargo.sh`) and
  produces the `lushtext` binary, with no duplicate cargo invocation defined
  separately in `snapcraft.yaml`

#### Scenario: No Rust source changes are required for the Snap
- **WHEN** the Snap is built from the current source tree
- **THEN** it succeeds without modifying any `.rs` file, relying on the existing
  `LUSHTEXT_PKGDATADIR` compile-time seam

### Requirement: Confined Resource And Schema Loading Via PKGDATADIR And Layout
The Snap SHALL make the binary's compile-time `LUSHTEXT_PKGDATADIR` path resolve
to the installed data inside strict confinement by bind-mounting that path to its
real `$SNAP` location using a snap `layout:`, so `register_resources()` and the
system GSettings schema directory work without code changes.

#### Scenario: GResource loads inside the confined snap
- **WHEN** the installed Snap launches under strict confinement
- **THEN** the binary loads `lushtext.gresource` from `LUSHTEXT_PKGDATADIR`
  successfully and does not panic with a missing-resource error

#### Scenario: App GSettings schema is available inside the confined snap
- **WHEN** the confined app reads or writes a `dev.cominotti.lushtext` GSettings
  key
- **THEN** the schema resolves from the snap's system schema directory and the
  operation succeeds

#### Scenario: Layout maps the baked path to the snap data location
- **WHEN** `snapcraft.yaml` is inspected
- **THEN** a `layout:` entry bind-mounts the baked `LUSHTEXT_PKGDATADIR` path to
  its corresponding `$SNAP` location

### Requirement: GTK 4.22 Platform Dependency Is Explicitly Gated
The Snap definition SHALL obtain the GTK 4.22 / Libadwaita 1.9 / GtkSourceView
runtime from a GNOME platform snap (the `core26` / GNOME 50 stack) and SHALL NOT
lower LushText's GNOME 50 feature floor or vendor the GNOME platform stack from
source.

#### Scenario: Platform floor is satisfied by the GNOME platform snap
- **WHEN** the Snap is built once the `core26` / GNOME 50 platform snap is
  available
- **THEN** the resulting Snap runs against GTK 4.22, Libadwaita 1.9, and
  GtkSourceView matching the project's GNOME 50 floor

#### Scenario: Platform unavailability does not force a workaround
- **WHEN** the GNOME 50 platform snap is not yet published
- **THEN** the change does not introduce a from-source GNOME build part and does
  not reduce the app's required GTK/Libadwaita versions

### Requirement: Snap Packaging Is Documented
The repository documentation SHALL describe the Snap build, the platform
availability gate, and how the Snap relates to the existing Flatpak build.

#### Scenario: Build documentation covers the Snap
- **WHEN** `README.md` and `.agents/rules/build.md` are reviewed after this change
- **THEN** they describe building the Snap locally, the `core26` / GNOME 50
  platform-snap gate, and that the Snap reuses the Meson/Cargo pipeline
