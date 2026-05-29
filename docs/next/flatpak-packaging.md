# Flatpak Packaging

## Status

LushText has an active Flatpak packaging path built with Meson and
`flatpak-builder`.

## Runtime/SDK

- Runtime: `org.gnome.Platform` 50
- SDK: `org.gnome.Sdk` 50
- Rust SDK extension: `org.freedesktop.Sdk.Extension.rust-stable`
- Manifest: `build-aux/dev.cominotti.lushtext.Flatpak.json`
- Local install command: `make flatpak-install`

`make flatpak-install` is idempotent for local setup: it adds the user
Flathub remote with `flatpak remote-add --if-not-exists --user` and asks
`flatpak-builder` to install the manifest's missing runtime, SDK, and SDK
extensions from that remote before building. `make flatpak` uses the same
dependency setup path for build-only validation.

## Flathub Publication

The local manifest at `build-aux/dev.cominotti.lushtext.Flatpak.json` is a
checkout-build manifest and intentionally keeps a local `type: "dir"` source.
Flathub publication uses a generated manifest under `build-aux/flathub/` that
replaces that source with an immutable public Git source:

```bash
make flathub-manifest VERSION=v0.2.0
make verify-flathub-manifest
```

The generated Flathub manifest preserves the reviewed local packaging contract:
app ID, command, GNOME runtime/SDK version, Rust SDK extension, Meson release
profile, finish arguments, cleanup rules, and vendored Cargo sources. It also
sets `CARGO_NET_OFFLINE=true` and copies `build-aux/cargo-sources.json` beside
the generated manifest so Flathub builds do not fetch Cargo dependencies during
the build.

The release workflow opens or updates a pull request against the configured
Flathub manifest repository when `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are
available. Human review and manual smoke testing remain the default publication
gate. Do not add `flathub.json` with `automerge-flathubbot-prs` unless a later
explicit policy change accepts the risk that a successful build does not prove
the app launches or preserves workspace behavior.

## Release Automation

Use dry runs while planning a release:

```bash
make release-bump TYPE=patch DRY_RUN=1
```

Use real release commands only from a clean `main` branch:

```bash
make release VERSION=v0.2.0 RELEASE_NOTES_FILE=release-notes.md
make release-bump TYPE=minor RELEASE_NOTES_FILE=release-notes.md
```

The release helper updates:

1. `meson.build`
2. `crates/lushtext/Cargo.toml`
3. `crates/lushtext-core/Cargo.toml`
4. `Cargo.lock`
5. `data/dev.cominotti.lushtext.metainfo.xml.in`
6. `build-aux/cargo-sources.json`

Real releases require deterministic release notes via `RELEASE_NOTES_FILE`; the
notes become the new AppStream release description. The helper validates version
surface consistency, AppStream metadata, generated desktop metadata, vendored
Cargo sources, and the Flatpak build before creating the release commit and
signed tag.

## Flathub Verification

The app ID `dev.cominotti.lushtext` is a custom-domain reverse-DNS ID. Flathub
therefore verifies the domain `cominotti.dev`; linking a GitHub account does not
verify this app ID.

After the app exists in Flathub's Developer Portal, request the verification
token there and publish it at:

```text
https://cominotti.dev/.well-known/org.flathub.VerifiedApps.txt
```

The file may contain multiple tokens, one per line, and comments beginning with
`#` are ignored by Flathub. Verify readiness locally with:

```bash
make verify-flathub-domain FLATHUB_VERIFICATION_TOKEN=<token>
```

This check fails on DNS/TLS problems, unreachable well-known files, or missing
tokens before the maintainer asks Flathub to verify the app.

## Desktop Identity

The production desktop identity is `dev.cominotti.lushtext.desktop`. The
installed Flatpak export must contain:

```desktop
X-Flatpak=dev.cominotti.lushtext
```

GNOME Settings uses this Flatpak identity to associate the app with sandbox
metadata. A same-ID non-Flatpak desktop file in
`$XDG_DATA_HOME/applications/dev.cominotti.lushtext.desktop` can shadow the
Flatpak export and make Settings treat LushText as an unsandboxed host app.

Development runs may temporarily stage the production desktop ID so GNOME Shell
can associate the debug process with the app while it is running. Normal
development staging must clean that file up. Persistent development staging must
use a non-production ID, currently `dev.cominotti.lushtext.Devel`, so it does
not override the installed Flatpak app info.

Run this check after local packaging or desktop-entry work:

```bash
make verify-flatpak-identity
```

The check verifies the Flatpak export marker, reports effective Flatpak
permissions, detects same-ID non-Flatpak shadow entries, and confirms the MIME
handler rows for plain text, Markdown, and empty documents remain registered.

## Permissions

Current manifest permissions:

- `--socket=wayland`
- `--socket=fallback-x11`
- `--share=ipc`
- `--device=dri`
- `--filesystem=host`

The display, IPC, and GPU permissions are the standard GTK/Libadwaita desktop
surface needed for hardware-accelerated rendering on Wayland with X11 fallback.

The broad `host` filesystem permission is intentionally retained for the
current workspace model and live-monitoring contract. LushText persists
workspace root paths and then uses them for sidebar tree loading, file watches,
command-palette indexing, workspace search and replace, sidecar notes, local
history, draft/session recovery, and in-app file operations. LushText must also
support event-driven external-change monitoring for user-selected files and
directories outside the home directory, not only one-off portal opens.

This permission is broader than the long-term ideal and must remain an explicit
product decision, not an accidental default. Flatpak guidance prefers portals
and narrower static filesystem permissions where possible. The portal-first
exploration found that document-portal paths can support app-initiated
read/write/rename/delete operations, but `gio monitor` and low-level inotify
probes did not receive events for host-side changes made to the original path.
Until LushText can preserve event-driven monitoring through a narrower model,
removing broad filesystem access would break required workspace behavior.

The current exploration note for that migration lives in
`docs/next/portal-first-sandbox-migration.md`. It splits the work into three
future spec-sized phases: portal-backed grants, portal-compatible workspace
behavior, and any later permission-tightening decision if event-driven behavior
can be preserved.

## GNOME Text Editor Comparison

GNOME Text Editor is useful as a feature and MIME reference. LushText now
matches its broad local filesystem posture for the specific reason that LushText
requires event-driven monitoring for local workspace paths outside the home
directory. LushText still should not copy unrelated GVfs or file-manager bus
permissions by default. Add GVfs or file-manager access only when a concrete
LushText workflow proves it is required and the reason is documented with the
manifest change.

## Required Files

1. `meson.build` - Meson build system wrapping Cargo
2. `meson_options.txt` - release/development profile option
3. `build-aux/cargo.sh` - Cargo wrapper for Meson builds
4. `build-aux/dev.cominotti.lushtext.Flatpak.json` - Flatpak manifest
5. `data/meson.build` - installs desktop file, metainfo, icons, and GSettings schema
6. `data/dev.cominotti.lushtext.desktop.in` - production desktop metadata
7. `data/dev.cominotti.lushtext.metainfo.xml.in` - AppStream metadata
8. `data/icons/` - app icons
9. `resources/meson.build` - compiles GResources via Meson
10. `po/meson.build` and `po/POTFILES` - i18n scaffolding

## Dependency Vendoring

Run this after Cargo dependency changes and before Flatpak builds:

```bash
make cargo-sources
```

This regenerates `build-aux/cargo-sources.json` so the Flatpak build can run
with vendored Cargo sources.
