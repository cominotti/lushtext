## 1. Desktop Metadata Allowlist

- [x] 1.1 Update `data/dev.cominotti.lushtext.desktop.in` so `MimeType` is exactly `text/plain;application/x-zerosize;application/json;application/json5;application/toml;application/yaml;text/markdown;`.
- [x] 1.2 Confirm the desktop entry no longer advertises `text/x-csrc`, `text/x-chdr`, `text/x-python`, or `text/x-rust`.
- [x] 1.3 Confirm JSONC and Properties MIME strings are not added by this change.

## 2. Development And Flatpak Export Paths

- [x] 2.1 Verify `scripts/run-dev-app.sh` still stages development desktop entries from `data/dev.cominotti.lushtext.desktop.in` and refreshes the applications desktop database after staging and restore.
- [x] 2.2 Update development desktop-staging tests or fixtures so the staged `MimeType` contract is checked against the exact curated allowlist.
- [x] 2.3 Update Flatpak identity verifier logic and fixtures so `make verify-flatpak-identity` checks all curated MIME types and rejects removed source MIME types in the active Flatpak desktop export.

## 3. Metadata Validation

- [x] 3.1 Run the desktop metadata validation path for the generated `.desktop` file and confirm `desktop-file-validate` succeeds.
- [x] 3.2 Refresh or rebuild the local development desktop export and confirm the generated `MimeType` field matches the curated allowlist.
- [x] 3.3 Refresh or rebuild the Flatpak/exported desktop entry and confirm the active export contains the same curated allowlist.

## 4. GLib And GNOME Settings Verification

- [x] 4.1 Run `gio mime text/plain`, `gio mime application/x-zerosize`, `gio mime application/json`, `gio mime application/json5`, `gio mime application/toml`, `gio mime application/yaml`, and `gio mime text/markdown` after cache refresh and confirm each lists `dev.cominotti.lushtext.desktop` as registered and recommended.
- [x] 4.2 Confirm installing, building, or staging LushText does not write user defaults to `mimeapps.list` or make LushText the default handler automatically.
- [x] 4.3 Confirm the explicit GNOME Settings File Types source data for LushText contains only Plain Text, Empty, JSON, JSON5, TOML, YAML, and Markdown document handlers; inherited `text/plain` subtype associations from `gio mime` are not treated as explicit File Types rows.

## 5. Regression Checks

- [x] 5.1 Run `make test-dev-desktop-staging`.
- [x] 5.2 Run `make test-flatpak-identity-verifier`.
- [x] 5.3 Run `make verify-flatpak-identity` when an installed Flatpak export is available.
- [x] 5.4 Run `openspec validate curate-desktop-document-handlers`.
