---
name: flatpak-rust
description: "Guide Flatpak packaging and Flathub publishing for Rust + GTK4/Libadwaita applications. Trigger whenever the user discusses packaging, distribution, Flatpak, Flathub, Meson build system, desktop files, AppStream metainfo, icons, app store submission, or works on files in build-aux/, data/, or the root meson.build. Also trigger when adding dependencies (cargo-sources.json regeneration needed), preparing releases, writing CI/CD pipelines for Flatpak builds, or when any file matching *.desktop*, *.metainfo*, *.Flatpak.json, or meson* is created or modified."
---

Guide the full Flatpak packaging pipeline for LushText — from Meson build system integration through Flathub publication. This skill covers the GNOME ecosystem conventions for Rust + GTK4/Libadwaita apps, drawing on patterns from established GNOME apps like GNOME Text Editor, Fractal, and Amberol.

The Flatpak build wraps Cargo inside Meson. Meson handles install targets, GResource compilation, desktop file installation, and AppStream metainfo — things Cargo doesn't know about. Flatpak then builds the whole thing in a sandboxed environment with vendored dependencies.

## Architecture Overview

```
                     ┌─────────────┐
                     │  Flathub CI  │
                     └──────┬──────┘
                            │ builds
                     ┌──────▼──────┐
                     │   Flatpak    │
                     │   Builder    │
                     └──────┬──────┘
                            │ invokes
                     ┌──────▼──────┐         ┌──────────────┐
                     │    Meson     │────────►│  cargo.sh    │
                     │  (root)      │         │  (wrapper)   │
                     └──────┬──────┘         └──────┬───────┘
                            │                       │ invokes
                     ┌──────▼──────┐         ┌──────▼───────┐
                     │  data/       │         │    Cargo      │
                     │  meson.build │         │  (Rust build) │
                     └─────────────┘         └──────────────┘
```

## File Checklist

All files required for a complete Flatpak package. Create in this order:

| # | File | Purpose | Blocks |
|---|------|---------|--------|
| 1 | `meson.build` (root) | Top-level Meson build, project metadata, subdir declarations | Everything |
| 2 | `meson_options.txt` | Build options (profile: debug/release) | `cargo.sh` |
| 3 | `build-aux/cargo.sh` | Shell wrapper: invokes Cargo inside Meson's sandbox | Meson build |
| 4 | `resources/meson.build` | GResource compilation via Meson (replaces `build.rs` for Flatpak) | Binary |
| 5 | `data/dev.cominotti.lushtext.desktop.in` | Desktop entry file | Desktop integration |
| 6 | `data/dev.cominotti.lushtext.metainfo.xml.in` | AppStream store metadata | Flathub listing |
| 7 | `data/icons/` | App icon (scalable SVG + symbolic) | Desktop integration |
| 8 | `data/meson.build` | Install desktop file, metainfo, icons, GSettings schema | Desktop integration |
| 9 | `build-aux/dev.cominotti.lushtext.Flatpak.json` | Flatpak manifest (modules, SDK, permissions) | Flatpak build |
| 10 | `po/meson.build` + `po/POTFILES` | i18n scaffolding (even if no translations yet) | Meson build |

## File Details

### 1. Root `meson.build`

```meson
project('lushtext',
  version: '0.1.0',
  meson_version: '>= 0.62.0',
  license: 'GPL-3.0-or-later',
)

i18n = import('i18n')
gnome = import('gnome')

# Build profile
profile = get_option('profile')
if profile == 'development'
  app_id = 'dev.cominotti.lushtext.Devel'
  vcs_tag = run_command('git', 'rev-parse', '--short', 'HEAD', check: false).stdout().strip()
else
  app_id = 'dev.cominotti.lushtext'
  vcs_tag = ''
endif

# Subdirectories
subdir('resources')
subdir('data')
subdir('po')

# Cargo build via wrapper script
cargo = find_program('build-aux/cargo.sh')
cargo_build = custom_target('cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: 'lushtext',
  console: true,
  command: [
    cargo,
    meson.project_build_root(),
    meson.project_source_root(),
    '@OUTPUT@',
    profile,
  ],
)

# Install binary
install_data(
  cargo_build,
  install_dir: get_option('bindir'),
  install_mode: 'rwxr-xr-x',
)
```

### 2. `meson_options.txt`

```meson
option('profile',
  type: 'combo',
  choices: ['development', 'release'],
  value: 'development',
  description: 'Build profile'
)
```

### 3. `build-aux/cargo.sh`

This is the crucial bridge between Meson and Cargo. It handles the Flatpak build environment:

```bash
#!/bin/sh
# Cargo wrapper for Meson builds — handles Flatpak sandbox constraints

export MESON_BUILD_ROOT="$1"
export MESON_SOURCE_ROOT="$2"
export CARGO_TARGET_DIR="$MESON_BUILD_ROOT/target"
export CARGO_HOME="${CARGO_HOME:-$MESON_BUILD_ROOT/cargo-home}"

OUTPUT="$3"
PROFILE="$4"

if [ "$PROFILE" = "release" ]; then
    CARGO_PROFILE="--release"
    TARGET_SUBDIR="release"
else
    CARGO_PROFILE=""
    TARGET_SUBDIR="debug"
fi

# Build the binary
cargo build --manifest-path "$MESON_SOURCE_ROOT/Cargo.toml" $CARGO_PROFILE -p lushtext

# Copy binary to Meson output
cp "$CARGO_TARGET_DIR/$TARGET_SUBDIR/lushtext" "$OUTPUT"
```

### 4. Flatpak Manifest

```json
{
    "id": "dev.cominotti.lushtext",
    "runtime": "org.gnome.Platform",
    "runtime-version": "50",
    "sdk": "org.gnome.Sdk",
    "sdk-extensions": ["org.freedesktop.Sdk.Extension.rust-stable"],
    "command": "lushtext",
    "finish-args": [
        "--socket=wayland",
        "--socket=fallback-x11",
        "--share=ipc",
        "--device=dri",
        "--filesystem=home"
    ],
    "build-options": {
        "append-path": "/usr/lib/sdk/rust-stable/bin",
        "env": {
            "CARGO_REGISTRIES_CRATES_IO_PROTOCOL": "sparse",
            "CARGO_HOME": "/run/build/lushtext/cargo"
        }
    },
    "cleanup": [
        "/include",
        "/lib/pkgconfig",
        "*.la",
        "*.a"
    ],
    "modules": [
        {
            "name": "lushtext",
            "buildsystem": "meson",
            "config-opts": ["-Dprofile=release"],
            "sources": [
                {
                    "type": "dir",
                    "path": ".."
                },
                "build-aux/cargo-sources.json"
            ]
        }
    ]
}
```

### 5. Desktop File

```desktop
[Desktop Entry]
Name=LushText
Comment=A minimalist text editor
Exec=lushtext %U
Icon=dev.cominotti.lushtext
Terminal=false
Type=Application
Categories=TextEditor;Utility;GTK;GNOME;
Keywords=Text;Editor;Code;
MimeType=text/plain;text/x-csrc;text/x-chdr;text/x-python;application/json;text/markdown;text/x-rust;
StartupNotify=true
# Translators: Do not translate this
X-Purism-FormFactor=workstation;
```

### 6. AppStream Metainfo

Read `references/appstream.md` for the complete template. Key requirements:
- `<id>` must match the app ID exactly
- `<launchable>` must match the desktop file name
- At least one `<screenshot>` with a caption
- `<content_rating>` must be present (use `oars-1.1`)
- `<releases>` section with at least one release
- `<branding>` with accent colors (GNOME 45+ convention)

### 7. Dependency Vendoring

Flatpak builds are **offline** — no network access during build. All Cargo dependencies must be pre-fetched:

```bash
# Install the generator
pip install flatpak-cargo-generator

# Generate cargo-sources.json from Cargo.lock
python3 -m flatpak_cargo_generator Cargo.lock -o build-aux/cargo-sources.json
```

**When to regenerate**: After any `cargo update`, adding/removing dependencies, or changing feature flags. The Flatpak build will fail if `cargo-sources.json` doesn't match `Cargo.lock`.

## Permissions Philosophy

Follow the principle of least privilege. LushText needs:

| Permission | Why | Can we narrow it? |
|------------|-----|-------------------|
| `--filesystem=home` | Text editor reads/writes user files | Could use portal API for file access (`--filesystem=host:ro` + portal), but this limits UX significantly for a text editor |
| `--socket=wayland` | Display on Wayland | Required |
| `--socket=fallback-x11` | Display on X11 (legacy) | Can drop when X11 support is no longer needed |
| `--share=ipc` | X11 shared memory (for fallback-x11) | Drop when X11 is dropped |
| `--device=dri` | GPU rendering (OpenGL/Vulkan for GTK4) | Required for hardware acceleration |

**Do NOT add**: `--share=network` (text editor doesn't need network), `--filesystem=host` (too broad), `--talk-name=org.freedesktop.*` (only if using specific D-Bus services).

## Flathub Submission Checklist

Read `references/flathub-review.md` for the full review criteria. The quick checklist:

1. [ ] App ID follows reverse DNS: `dev.cominotti.lushtext`
2. [ ] Desktop file validates: `desktop-file-validate data/*.desktop.in`
3. [ ] AppStream metainfo validates: `appstreamcli validate data/*.metainfo.xml.in`
4. [ ] At least one screenshot in metainfo (1602x900px recommended)
5. [ ] Content rating present (OARS 1.1)
6. [ ] License is FOSS (GPL-3.0-or-later ✓)
7. [ ] No bundled libraries that are in the runtime
8. [ ] Minimal permissions (no `--filesystem=host`, no `--share=network` unless needed)
9. [ ] `cargo-sources.json` committed and up-to-date
10. [ ] `appstreamcli validate --explain` passes with no errors
11. [ ] Build succeeds with `flatpak-builder --force-clean build-dir build-aux/*.Flatpak.json`
12. [ ] App launches and basic functionality works in sandbox

## CI Integration

### GitHub Actions for Flatpak Build

```yaml
name: Flatpak
on:
  push:
    branches: [main]
  pull_request:

jobs:
  flatpak:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/flathub-infra/flatpak-github-actions:gnome-50
      options: --privileged
    steps:
      - uses: actions/checkout@v4
      - uses: flatpak/flatpak-github-actions/flatpak-builder@v6
        with:
          manifest-path: build-aux/dev.cominotti.lushtext.Flatpak.json
          bundle: lushtext.flatpak
          cache-key: flatpak-builder-${{ github.sha }}
```

This CI job:
- Builds the Flatpak in a container matching the runtime version
- Produces a `.flatpak` bundle as an artifact
- Caches the build for faster subsequent runs

## Common Pitfalls

### Cargo.lock Must Be Committed

Flatpak needs `Cargo.lock` to generate `cargo-sources.json`. Without it, the vendoring step fails. Always commit `Cargo.lock` for binary crates.

### GResource Compilation in Meson vs `build.rs`

For development builds, `build.rs` compiles GResources via `glib_build_tools`. For Flatpak builds, Meson compiles them via `gnome.compile_resources()`. The binary needs to handle both:

```rust
// In lib.rs — register resources from either source
fn register_resources() {
    // Try Meson-compiled resources first (Flatpak/installed build)
    let resource_path = "/dev/cominotti/lushtext/";
    if gio::resources_lookup_data(
        &format!("{resource_path}ui/window.ui"),
        gio::ResourceLookupFlags::NONE,
    ).is_ok() {
        return; // Already registered by Meson/GResource system
    }
    
    // Fall back to build.rs-compiled resources (dev build)
    let bytes = glib::Bytes::from_static(
        include_bytes!(concat!(env!("OUT_DIR"), "/lushtext.gresource"))
    );
    let resource = gio::Resource::from_data(&bytes).unwrap();
    gio::resources_register(&resource);
}
```

### SDK Extension Path

The Rust SDK extension installs to `/usr/lib/sdk/rust-stable/bin`. The manifest must add this to `PATH` via `build-options.append-path`, or `cargo` won't be found.

### Profile Mismatch

Dev builds use debug profile; Flatpak should use release. The `cargo.sh` wrapper reads the Meson profile option to pass `--release` to Cargo. Forgetting this results in a debug build in the Flatpak (slow, large binary).
