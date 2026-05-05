## 1. Identity Diagnosis And Verification Helpers

- [x] 1.1 Add a deterministic verification path that reports the production Flatpak export for `dev.cominotti.lushtext.desktop` and confirms it contains `X-Flatpak=dev.cominotti.lushtext`.
- [x] 1.2 Make verification fail with a clear message when `~/.local/share/applications/dev.cominotti.lushtext.desktop` exists as a same-ID non-Flatpak entry that can shadow the Flatpak export.
- [x] 1.3 Include effective permission reporting from `flatpak info --show-permissions dev.cominotti.lushtext` in the verification output.
- [x] 1.4 Verify the Flatpak metadata query confirms `dev.cominotti.lushtext` is installed as a Flatpak app with command `lushtext`.

## 2. Development Desktop Entry Coexistence

- [x] 2.1 Update `scripts/run-dev-app.sh` so a normal development run does not leave a persistent same-ID production desktop entry after cleanup.
- [x] 2.2 Ensure any explicit keep-staged development workflow uses a non-production identity or another verified approach that does not shadow the Flatpak-backed production app info.
- [x] 2.3 Refresh desktop and icon caches after staging and cleanup so GLib/GNOME Shell do not keep stale same-ID metadata.
- [x] 2.4 Add or update tests/checks for dev-run desktop-entry staging so the production ID shadowing regression is caught.

## 3. Flatpak Permission Review And Documentation

- [x] 3.1 Review `build-aux/dev.cominotti.lushtext.Flatpak.json` permissions against current workspace, file dialog, search, watcher, notes, local-history, rendering, and desktop-integration behavior.
- [x] 3.2 Compare LushText's permissions with GNOME Text Editor's current Flatpak permission set without broadening LushText permissions unless verification proves a need.
- [x] 3.3 Update the manifest only for permissions that are required and justified by current behavior.
- [x] 3.4 Refresh `docs/next/flatpak-packaging.md` with the GNOME 50 runtime target, current permission rationale, portal guidance, and the portal-first workspace follow-up.

## 4. Files And Links Regression Coverage

- [x] 4.1 Verify `gio mime text/plain`, `gio mime text/markdown`, and `gio mime application/x-zerosize` still list `dev.cominotti.lushtext.desktop` as registered and recommended after desktop cache refresh.
- [x] 4.2 Verify the Files & Links acceptance path uses the Flatpak-backed app info when the Flatpak is installed, not a stale same-ID development desktop entry.
- [x] 4.3 Confirm the change does not write user defaults to `mimeapps.list` or make LushText the default handler automatically.

## 5. End-To-End Packaging Validation

- [x] 5.1 Build or refresh the local Flatpak package/exported desktop metadata from the current checkout.
- [x] 5.2 Run desktop metadata validation, OpenSpec validation, and the new Flatpak identity verification path.
- [x] 5.3 Launch through `gtk-launch dev.cominotti.lushtext` and confirm it exercises the Flatpak export when the Flatpak is installed.
- [x] 5.4 Inspect GNOME Settings > Apps > LushText, or an equivalent app-info probe, and confirm the app is no longer treated as an unsandboxed host application because of missing Flatpak identity.
