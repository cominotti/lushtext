## Why

GNOME Settings can show LushText as "App is not sandboxed" even when the Flatpak package is installed, because a same-ID development desktop entry can shadow the Flatpak export and lacks the `X-Flatpak` marker Settings uses to resolve sandbox metadata. This also exposes a broader packaging gap: LushText's filesystem permission posture should be explicit, verified, and documented instead of inheriting stale "text editor needs home access" assumptions.

## What Changes

- Ensure GNOME resolves the shipped `dev.cominotti.lushtext.desktop` app info through the Flatpak export when the Flatpak is installed, including the `X-Flatpak=dev.cominotti.lushtext` marker.
- Prevent local development desktop-entry staging from persistently shadowing the Flatpak export under the same desktop ID.
- Add deterministic verification for Flatpak identity, desktop-entry precedence, Settings-visible sandbox metadata, and MIME handler visibility.
- Review the Flatpak manifest against GNOME/Flatpak best practices and GNOME Text Editor's current permission set, then document which permissions LushText intentionally keeps for its workspace model.
- Refresh the stale packaging note so future work does not repeat the outdated blanket `--filesystem=home` rationale.

## Capabilities

### New Capabilities

- `flatpak-sandbox-identity`: Covers LushText's Flatpak desktop identity, development desktop-entry coexistence, static permission posture, and Settings-visible sandbox metadata.

### Modified Capabilities

- `desktop-document-handlers`: Adds the requirement that the GNOME Files & Links handler surface must remain visible through the Flatpak-backed app info and must not be hidden by a stale same-ID development desktop entry.

## Impact

- Affected files: `scripts/run-dev-app.sh`, `build-aux/dev.cominotti.lushtext.Flatpak.json`, `docs/next/flatpak-packaging.md`, and OpenSpec desktop integration specs.
- Affected systems: GNOME Settings app details, GLib/GIO app-info lookup, Flatpak exported desktop metadata, local development desktop-entry staging, desktop MIME cache refresh, and Flatpak static permission review.
- Verification will include local `GAppInfo`/desktop-entry checks, `flatpak info --show-permissions`, active export inspection, MIME registration checks, and Flatpak packaging validation.
