# Flathub Review Criteria

What the Flathub reviewers check when you submit an app. Address all of these before creating a pull request to the flathub GitHub organization.

## Submission Process

1. Fork `flathub/flathub` on GitHub
2. Create a new branch with your app ID
3. Add your manifest as `dev.cominotti.lushtext.json` (NOT `.Flatpak.json` — Flathub uses plain `.json`)
4. Open a pull request
5. Flathub CI builds and tests your app
6. A reviewer checks the criteria below

## Mandatory Requirements

### App ID
- Must be a valid reverse DNS name: `dev.cominotti.lushtext` ✓
- Must own or control the domain (or use a GitHub-based ID like `io.github.cominotti.lushtext`)
- Desktop file, metainfo, and icon filenames must match the app ID exactly

### Manifest Quality
- No `--filesystem=host` (too broad — use `--filesystem=home` or portals)
- No `--share=network` unless the app genuinely needs internet access
- No `--socket=session-bus` or `--socket=system-bus` (use specific `--talk-name` instead)
- No `--device=all` (use specific devices like `--device=dri`)
- Release profile must be used (`-Dprofile=release` or equivalent)
- No debug symbols in the final binary (Cargo `strip = true` in release profile ✓)

### AppStream Metainfo
- Must validate: `appstreamcli validate --explain --pedantic`
- Required elements: `<id>`, `<name>`, `<summary>`, `<description>`, `<launchable>`, `<url type="homepage">`, `<content_rating>`, `<releases>`, `<developer>`
- At least one `<screenshot>` with `<caption>` — screenshots should show the app in action
- Screenshot dimensions: 1602x900px (16:9) recommended, min 624px wide
- `<content_rating type="oars-1.1">` must be present (empty element = all-ages)
- `<releases>` must have at least one `<release>` with version and date

### Desktop File
- Must validate: `desktop-file-validate`
- `Categories` must include a valid main category (e.g., `TextEditor`)
- `Exec` line should use `%U` for URI handling
- `Icon` must match the app ID
- `StartupNotify=true` for GTK4 apps

### Icons
- Scalable SVG icon required: `data/icons/dev.cominotti.lushtext.svg`
- Symbolic icon recommended: `data/icons/dev.cominotti.lushtext-symbolic.svg`
- Icon must be installable to `hicolor` theme
- Should look good at 128x128px (the Software Center size)

### Build
- Must build reproducibly from the manifest
- `cargo-sources.json` must be included and up-to-date
- No network access during build (all dependencies vendored)
- Build must succeed on x86_64 at minimum

## Common Rejection Reasons

1. **Missing content rating**: Add `<content_rating type="oars-1.1"/>` to metainfo
2. **Screenshots missing or broken**: URLs must be HTTPS, images must be hosted reliably
3. **Overly broad permissions**: `--filesystem=host` instead of `--filesystem=home`
4. **Stale cargo-sources.json**: Doesn't match current `Cargo.lock`
5. **Missing developer info**: `<developer>` element required in metainfo
6. **No releases**: At least one `<release>` with version and date
7. **Debug build**: Forgot to set release profile in manifest
8. **Bundled libraries**: Including libs already in `org.gnome.Platform` (e.g., GTK4, libadwaita)

## Flathub Manifest Differences

The Flathub manifest differs slightly from a local development manifest:

```json
{
    "id": "dev.cominotti.lushtext",
    "runtime": "org.gnome.Platform",
    "runtime-version": "49",
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
    "modules": [
        {
            "name": "lushtext",
            "buildsystem": "meson",
            "config-opts": ["-Dprofile=release"],
            "sources": [
                {
                    "type": "git",
                    "url": "https://github.com/cominotti/lushtext.git",
                    "tag": "v0.1.0",
                    "commit": "<full-sha>"
                },
                "cargo-sources.json"
            ]
        }
    ]
}
```

Key differences:
- Source is `type: "git"` with a tag (not `type: "dir"`)
- `cargo-sources.json` is referenced inline (Flathub hosts it in the same repo)
- No `cleanup` needed (Flathub handles this)
