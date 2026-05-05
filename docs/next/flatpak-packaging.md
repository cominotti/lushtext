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
- `--filesystem=home`

The display, IPC, and GPU permissions are the standard GTK/Libadwaita desktop
surface needed for hardware-accelerated rendering on Wayland with X11 fallback.

The broad `home` filesystem permission is intentionally retained for the
current workspace model. LushText persists workspace root paths and then uses
them for sidebar tree loading, file watches, command-palette indexing,
workspace search and replace, sidecar notes, local history, draft/session
recovery, and in-app file operations. Removing broad filesystem access without
a portal-backed workspace grant model would break restored workspaces and
background workspace features after restart.

This is still less sandboxed than the long-term ideal. Flatpak guidance prefers
portals and narrower static filesystem permissions where possible. A future
portal-first workspace design should investigate persisting document-portal
grants or another user-visible reauthorization flow so LushText can narrow or
remove broad `home` access without losing workspace behavior.

## GNOME Text Editor Comparison

GNOME Text Editor is useful as a feature and MIME reference, but its current
Flatpak permission set is broader than LushText's: it grants `host`,
`xdg-run/gvfsd`, `org.gtk.vfs.*`, and `org.freedesktop.FileManager1`.

LushText should not copy those broader permissions by default. Add GVfs,
file-manager, or `host` access only when a concrete LushText workflow proves it
is required and the reason is documented with the manifest change.

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
