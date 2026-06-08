# Meson + Cargo Integration for GTK4/Rust Apps

Detailed patterns for integrating Cargo builds with Meson, covering GResource compilation, install targets, and the dev/Flatpak build split.

## Why Meson Wraps Cargo

Cargo handles Rust compilation but doesn't know about:
- Desktop file installation (`/usr/share/applications/`)
- AppStream metainfo installation (`/usr/share/metainfo/`)
- Icon installation (`/usr/share/icons/hicolor/`)
- GSettings schema compilation and installation
- GResource compilation at install time (vs `build.rs` for dev)
- i18n/gettext integration

Meson fills these gaps. The Flatpak builder invokes Meson, which invokes Cargo via `cargo.sh`.

## GResource Dual-Path Strategy

**Dev builds** (Makefile / direct `cargo build`):
- `build.rs` calls `glib_build_tools::compile_resources()`
- Output: `$OUT_DIR/lushtext.gresource`
- Binary loads via `include_bytes!` + `gio::Resource::from_data()`

**Installed/Flatpak builds** (Meson):
- `resources/meson.build` calls `gnome.compile_resources()`
- Output: installed to standard GResource path
- Binary detects pre-registered resources and skips manual loading

### `resources/meson.build`

```meson
gnome = import('gnome')

blueprints = custom_target('blueprints',
  input: files(
    'ui/window.ui',
    'ui/editor-page.ui',
    'ui/sidebar.ui',
    'ui/search-bar.ui',
    'ui/preferences.ui',
    'ui/shortcuts.ui',
  ),
  output: '.',
  command: ['echo', 'UI files processed'],  # No-op if not using Blueprint
)

gnome.compile_resources('lushtext',
  'dev.cominotti.lushtext.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: get_option('datadir') / 'lushtext',
  dependencies: blueprints,
)
```

### Detection in `lib.rs`

```rust
pub fn run() {
    // Register GResources — handles both dev and installed builds
    register_resources();
    
    // ... rest of app setup
}

fn register_resources() {
    // Check if resources are already registered (Meson installed them)
    if gio::resources_lookup_data(
        "/dev/cominotti/lushtext/ui/window.ui",
        gio::ResourceLookupFlags::NONE,
    ).is_ok() {
        return;
    }
    
    // Dev build: load from build.rs output
    let bytes = glib::Bytes::from_static(
        include_bytes!(concat!(env!("OUT_DIR"), "/lushtext.gresource"))
    );
    let resource = gio::Resource::from_data(&bytes)
        .expect("Failed to load GResource bundle");
    gio::resources_register(&resource);
}
```

## `cargo.sh` Wrapper Details

The wrapper handles:
1. Setting `CARGO_TARGET_DIR` to Meson's build directory
2. Setting `CARGO_HOME` for the Flatpak sandbox
3. Selecting release vs debug profile
4. Copying the binary to Meson's expected output location

```bash
#!/bin/sh
set -eu

export MESON_BUILD_ROOT="$1"
export MESON_SOURCE_ROOT="$2"
export CARGO_TARGET_DIR="$MESON_BUILD_ROOT/target"
export CARGO_HOME="${CARGO_HOME:-$MESON_BUILD_ROOT/cargo-home}"

OUTPUT="$3"
PROFILE="$4"

if [ "$PROFILE" = "release" ]; then
    echo "Building in release mode..."
    cargo build --manifest-path "$MESON_SOURCE_ROOT/Cargo.toml" \
        --release -p lushtext
    cp "$CARGO_TARGET_DIR/release/lushtext" "$OUTPUT"
else
    echo "Building in debug mode..."
    cargo build --manifest-path "$MESON_SOURCE_ROOT/Cargo.toml" \
        -p lushtext
    cp "$CARGO_TARGET_DIR/debug/lushtext" "$OUTPUT"
fi
```

**Important**: The `--manifest-path` flag points to the repository-root `Cargo.toml`, and `-p lushtext` builds only the binary crate. This avoids building workspace-hack or other internal crates unnecessarily.

## `data/meson.build`

```meson
# Desktop file
desktop_file = i18n.merge_file(
  input: 'dev.cominotti.lushtext.desktop.in',
  output: 'dev.cominotti.lushtext.desktop',
  type: 'desktop',
  po_dir: '../po',
  install: true,
  install_dir: get_option('datadir') / 'applications',
)

# Validate desktop file
desktop_file_validate = find_program('desktop-file-validate', required: false)
if desktop_file_validate.found()
  test('validate-desktop-file',
    desktop_file_validate,
    args: [desktop_file],
  )
endif

# AppStream metainfo
metainfo_file = i18n.merge_file(
  input: 'dev.cominotti.lushtext.metainfo.xml.in',
  output: 'dev.cominotti.lushtext.metainfo.xml',
  po_dir: '../po',
  install: true,
  install_dir: get_option('datadir') / 'metainfo',
)

# Validate metainfo
appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test('validate-metainfo',
    appstreamcli,
    args: ['validate', '--no-net', '--explain', metainfo_file],
  )
endif

# Icons
install_data(
  'icons/dev.cominotti.lushtext.svg',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'scalable' / 'apps',
)
install_data(
  'icons/dev.cominotti.lushtext-symbolic.svg',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'symbolic' / 'apps',
)
```

## Regenerating cargo-sources.json

```bash
# Install the generator (once)
pip install flatpak-cargo-generator

# Regenerate after any dependency change
python3 -m flatpak_cargo_generator Cargo.lock \
    -o build-aux/cargo-sources.json

# The generated file is a JSON array of source objects that flatpak-builder
# uses to pre-fetch crates. It must be committed to the repo.
```

**Automation tip**: Add a Makefile target:
```makefile
cargo-sources: Cargo.lock
	python3 -m flatpak_cargo_generator Cargo.lock -o build-aux/cargo-sources.json
```
