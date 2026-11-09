## ADDED Requirements

### Requirement: The Flatpak ships the toolkit the test lanes run
The Flatpak manifest SHALL build LushText on the GNOME runtime whose toolkit the local development environment and the CI test, measurement, and lint containers run: `org.gnome.Platform` and `org.gnome.Sdk` 51 (GTK 4.24, GtkSourceView 5.22, Libadwaita 1.10). Its SDK and Rust extension SHALL follow that runtime. CI's Flatpak build and release jobs SHALL use the matching `flatpak-github-actions` image (`gnome-51`). A runtime change SHALL rebuild the Flatpak and pass `make verify-flatpak-identity` before it is accepted. It SHALL NOT change the app id, command, finish arguments, or permissions.

#### Scenario: Manifest runtime and test toolkit agree
- **WHEN** `build-aux/dev.cominotti.lushtext.Flatpak.json`, the CI workflows, and the documented development toolbox are inspected
- **THEN** the manifest names `runtime-version` 51, the Flatpak jobs use `flatpak-github-actions:gnome-51`, and the Fedora test containers carry GTK 4.24

#### Scenario: Identity survives the runtime move
- **WHEN** the GNOME 51 Flatpak is built and installed locally
- **THEN** `make verify-flatpak-identity` passes with the same app id, command, permissions, and MIME registration as on GNOME 50
