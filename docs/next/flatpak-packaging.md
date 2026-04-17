# Flatpak Packaging

## Status: Deferred to after core features are complete

## Description
Package lushtext as a Flatpak with proper desktop integration.

## Required Files
1. `meson.build` (root) — Meson build system wrapping cargo
2. `meson_options.txt` — devel/release profile option
3. `build-aux/cargo.sh` — cargo wrapper for Meson sandbox
4. `build-aux/dev.cominotti.lushtext.Flatpak.json` — Flatpak manifest
5. `data/meson.build` — install desktop file, metainfo, icons, GSettings schema
6. `data/dev.cominotti.lushtext.desktop.in` — desktop file
7. `data/dev.cominotti.lushtext.metainfo.xml.in` — AppStream metainfo
8. `data/icons/` — app icons (scalable SVG, symbolic, and PNG fallbacks)
9. `resources/meson.build` — compile GResources via Meson
10. `po/meson.build` + `po/POTFILES` — i18n scaffolding

## Runtime/SDK
- Runtime: `org.gnome.Platform` (version 48+)
- SDK: `org.gnome.Sdk`
- SDK extension: `org.freedesktop.Sdk.Extension.rust-stable`

## Permissions
- `--filesystem=home` (text editor needs file access)
- `--socket=wayland`, `--socket=fallback-x11`
- `--device=dri` (GPU rendering)
- `--share=ipc`

## Dependency Vendoring
Run `flatpak-cargo-generator.py Cargo.lock -o build-aux/cargo-sources.json`
before each Flatpak build to pre-fetch cargo dependencies.
