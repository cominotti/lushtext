## MODIFIED Requirements

### Requirement: Rust floor is aligned across all stable toolchain surfaces
The implementation SHALL pin LushText's stable Rust floor (MSRV) to Rust 1.98.0. It SHALL pin the toolchain to the Rust version that `org.freedesktop.Sdk.Extension.rust-stable` ships for the Freedesktop SDK branch of the Flatpak manifest's GNOME runtime: `26.08` for GNOME 51, which shipped Rust 1.98.0 on 2026-11-09. The pin SHALL be identical everywhere the repository declares, installs, bootstraps, or documents the stable toolchain, so the binary that is tested and measured is built by the compiler that builds the shipped binary. A runtime change SHALL re-check that extension's version and move the pin in the same change. The implementation MUST keep nightly-only tooling, such as coverage-guided fuzz smoke, explicitly separate from the stable MSRV.

#### Scenario: Stable version surfaces agree
- **WHEN** the repository is searched for active Rust MSRV, rustup, CI stable-toolchain, Snap rustup bootstrap, README, AGENTS, rule, and skill references
- **THEN** active references name Rust 1.98.0 as the floor and the shipped extension's Rust version as the pinned toolchain
- **AND** a previous pin (such as Rust 1.98.1 from the GNOME 50 SDK's `25.08` extension) remains only in archive/history context or is removed

#### Scenario: Nightly lanes remain explicit exceptions
- **WHEN** fuzz smoke or other nightly-only tooling is inspected
- **THEN** those lanes remain pinned to or invoked through nightly as required
- **AND** they are not presented as the stable Rust floor for normal development, CI, or packaging

#### Scenario: Packaging toolchains satisfy the floor
- **WHEN** Flatpak and Snap packaging surfaces are inspected
- **THEN** Snap's rustup bootstrap installs the pinned toolchain
- **AND** the Flatpak manifest's GNOME SDK provides a Rust stable extension whose version equals the pinned toolchain and satisfies Rust 1.98.0, or a concrete blocker is documented before implementation can be considered complete
