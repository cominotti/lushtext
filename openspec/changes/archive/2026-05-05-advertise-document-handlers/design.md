## Context

GNOME Settings exposes a per-application "Files & Links" section from GLib's `GAppInfo` supported content types. The row is not driven by LushText preferences or AppStream feature text; it is driven by the installed desktop entry's `MimeType` values and by the desktop MIME cache that `update-desktop-database` builds for the relevant applications directory.

LushText already advertises `text/plain` and `text/markdown` in `data/dev.cominotti.lushtext.desktop.in`, but it does not advertise `application/x-zerosize`, the freedesktop MIME type GLib reports for empty files. GNOME Text Editor uses `text/plain;application/x-zerosize;` as the minimal reference for plain text and empty documents. On this system, Markdown is canonicalized as `text/markdown`, with `text/x-markdown` present as a platform alias.

LushText also has two desktop-entry surfaces that can drift:

- The packaged Flatpak/exported desktop entry generated from `data/dev.cominotti.lushtext.desktop.in`.
- The development desktop entry staged by local run workflows so GNOME Shell and Settings can associate the running app with `dev.cominotti.lushtext.desktop`.

The change must keep those surfaces aligned and verifiable.

## Goals / Non-Goals

**Goals:**

- Make LushText visible in GNOME Settings > Apps > LushText > Files & Links for Plain Text, Markdown, and Empty document entries.
- Register LushText as a recommended handler for `text/plain`, `text/markdown`, and `application/x-zerosize`.
- Keep the canonical Markdown contract on `text/markdown`; rely on the system alias for `text/x-markdown` unless verification shows a target environment needs explicit compatibility.
- Refresh desktop MIME caches for both installed/Flatpak and local development exports.
- Verify the behavior through deterministic command-line checks before relying on visual GNOME Settings inspection.

**Non-Goals:**

- Do not add an in-app preferences page or a custom "make default app" button.
- Do not set LushText as a user's default handler during install or dev-run staging.
- Do not broaden LushText into a general handler for every text-like source code MIME type.
- Do not change document loading, Markdown preview, session restore, draft handling, or file I/O behavior.

## Decisions

### Decision 1: Use desktop metadata as the source of truth

Update the desktop entry's `MimeType` list to include the required document types. GNOME Settings reads `GAppInfo`, which in turn reads installed desktop entries and MIME caches, so metadata is the correct integration point.

Alternatives considered:

- Add app UI for default handlers. Rejected because GNOME already owns the standard UI, and a local control would duplicate platform behavior.
- Write directly to `mimeapps.list`. Rejected because installation must register support without mutating a user's current defaults.

### Decision 2: Add `application/x-zerosize` explicitly

`application/x-zerosize` is the freedesktop content type GLib reports for empty files. GNOME Text Editor advertises it, and GNOME Settings will only list LushText for Empty documents once the app is recommended for that type.

Alternatives considered:

- Use `inode/x-empty`. Rejected because `gio info` reports empty regular files as `application/x-zerosize` on the target desktop stack, and `gio mime inode/x-empty` has no local default/recommended applications.
- Rely on `text/plain` inheritance. Rejected because `application/x-zerosize` is not treated as a `text/plain` subclass in the shared MIME database.

### Decision 3: Keep Markdown canonical

Use `text/markdown` as the normative Markdown MIME type. The local shared MIME database maps `text/x-markdown` as an alias to `text/markdown`, so adding the alias to LushText's desktop entry is not required for the MVP.

Alternatives considered:

- Add both `text/markdown` and `text/x-markdown`. Deferred unless verification on supported targets shows `text/x-markdown` is not aliased. Keeping one canonical entry reduces duplicate rows and avoids muddying the contract.

### Decision 4: Verify caches, not just files

The implementation must validate generated desktop metadata and then verify GLib's view after cache refresh. The useful checks are `desktop-file-validate` against a generated `.desktop` file, `update-desktop-database` for the directory being exported, and `gio mime` checks for `text/plain`, `text/markdown`, and `application/x-zerosize`.

Alternatives considered:

- Inspect only `data/dev.cominotti.lushtext.desktop.in`. Rejected because stale exported desktop files or stale MIME caches can make the source file correct while GNOME Settings remains wrong.

## Risks / Trade-offs

- Stale desktop exports can hide a correct source change -> Mitigation: include verification against the active exported desktop entry and cache, not only source metadata.
- Development and Flatpak desktop entries can drift -> Mitigation: keep dev-run staging derived from the same source contract and add a task to refresh/verify both surfaces.
- `gio mime` can be affected by user-level `mimeapps.list` defaults -> Mitigation: acceptance checks only require LushText to appear as registered and recommended; they must not require LushText to become the default automatically.
- Platform MIME alias behavior may vary -> Mitigation: verify `text/markdown` first, and only add `text/x-markdown` explicitly if target-platform evidence shows the alias is insufficient.

## Migration Plan

1. Update the desktop entry MIME contract.
2. Ensure development desktop-entry staging mirrors the same MIME contract and refreshes the local applications cache.
3. Rebuild or refresh the Flatpak/exported app metadata.
4. Validate desktop and AppStream metadata.
5. Verify GLib registration for the three required document types.

Rollback is a metadata-only revert: remove the added MIME type from the desktop entry, refresh the same caches, and confirm `gio mime application/x-zerosize` no longer lists LushText as registered or recommended.

## Open Questions

None for the MVP.
