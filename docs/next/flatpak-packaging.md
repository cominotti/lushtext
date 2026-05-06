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
