## 1. Desktop Metadata Contract

- [x] 1.1 Update `data/dev.cominotti.lushtext.desktop.in` so `MimeType` includes `text/plain`, `text/markdown`, and `application/x-zerosize`.
- [x] 1.2 Preserve the existing declared source/document MIME handlers unless validation shows a concrete reason to narrow or reorder them.
- [x] 1.3 Confirm no install, build, or run script writes `dev.cominotti.lushtext.desktop` into `mimeapps.list` as a default handler.

## 2. Development And Packaged Export Alignment

- [x] 2.1 Verify `scripts/run-dev-app.sh` stages the development desktop entry from `data/dev.cominotti.lushtext.desktop.in` so the new MIME contract appears in `~/.local/share/applications/dev.cominotti.lushtext.desktop`.
- [x] 2.2 Refresh the desktop application cache for the development applications directory after staging if the current workflow does not already do so.
- [x] 2.3 Rebuild or refresh the Flatpak/exported desktop entry and confirm the active export contains the same `MimeType` contract.

## 3. Metadata And MIME Verification

- [x] 3.1 Run the Meson desktop-file validation path or an equivalent generated-file `desktop-file-validate` check against `dev.cominotti.lushtext.desktop`.
- [x] 3.2 Run AppStream validation to confirm the metadata change does not regress packaged app metadata.
- [x] 3.3 Run `gio mime text/plain`, `gio mime text/markdown`, and `gio mime application/x-zerosize` after cache refresh and confirm each lists `dev.cominotti.lushtext.desktop` as registered and recommended.
- [x] 3.4 Confirm the verification does not require LushText to become the current default handler for any of the three MIME types.

## 4. GNOME Settings Acceptance

- [x] 4.1 Inspect GNOME Settings > Apps > LushText > Files & Links, or an equivalent `GAppInfo` probe, and confirm Plain Text, Markdown, and Empty document entries are visible for LushText.
- [x] 4.2 Use the standard GNOME Settings or GLib MIME path to set LushText as the default for at least `application/x-zerosize`, then restore any previous user default captured before the check.
- [x] 4.3 Document any environment-specific cache refresh command needed for Flatpak or local development exports if it is not already encoded in the workflow.

## 5. OpenSpec And Final Checks

- [x] 5.1 Run `openspec validate advertise-document-handlers`.
- [x] 5.2 Run `git diff --check`.
- [x] 5.3 Record the exact validation commands and outcomes in the implementation summary.
