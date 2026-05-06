## Context

GNOME Settings exposes per-application file types from GLib's application association data. For LushText, that data comes from the installed or staged `.desktop` entry's `MimeType` field plus the desktop MIME cache refreshed by `update-desktop-database`; it is not driven by Rust editor behavior, AppStream prose, or GtkSourceView language support.

The current `desktop-document-handlers` spec guarantees Plain Text, Markdown, and Empty document visibility. The shipped desktop entry now advertises those types plus JSON and programming-language source types (`text/x-csrc`, `text/x-chdr`, `text/x-python`, and `text/x-rust`). The desired contract is narrower and more document-oriented: Plain Text, Empty, JSON, JSON5, TOML, YAML, and Markdown only.

JSONC and Properties are intentionally excluded from this proposal. On the target desktop stack used during exploration, `.jsonc` and `.properties` files resolve to generic `text/plain` unless additional shared-MIME definitions are introduced. This change avoids adding custom MIME packages and keeps the handler list limited to stable platform MIME types.

## Goals / Non-Goals

**Goals:**

- Make the desktop handler contract an exact allowlist instead of a minimum set.
- Advertise LushText for `text/plain`, `application/x-zerosize`, `application/json`, `application/json5`, `application/toml`, `application/yaml`, and `text/markdown`.
- Remove programming-language source MIME types from both packaged and development desktop entries.
- Keep the packaged Flatpak/exported desktop entry and local development staging derived from the same source contract.
- Verify generated desktop metadata, cache-refresh behavior, and GLib-visible registration for every allowed MIME type.
- Verify excluded source MIME types are absent from generated/staged metadata. GLib may still list LushText for some source-like types through `text/plain` inheritance, so verification must distinguish explicit desktop-supported types from inherited MIME associations.

**Non-Goals:**

- Do not add JSONC or Properties rows in this change.
- Do not install a custom shared-MIME package or MIME XML definition.
- Do not add an in-app default-application preferences UI.
- Do not write directly to `mimeapps.list` or make LushText the default handler automatically.
- Do not change document loading, syntax highlighting, Markdown preview, session restore, draft handling, or file I/O behavior.
- Do not remove LushText's ability to open source files when a user chooses them from the file picker or command line.

## Decisions

### Decision 1: Treat `MimeType` as an exact allowlist

The source desktop entry should contain only the MIME types this spec names. That makes GNOME Settings' File Types surface intentional and avoids presenting LushText as a code editor for language-specific source files.

Alternatives considered:

- Keep programming-language MIME types because LushText can open text files. Rejected because GNOME Settings file associations are a user-facing default-app contract, not a list of every format the editor can technically read.
- Move the allowlist into a separate generated source. Rejected because the existing single desktop template is already the source of truth for packaged and development staging.

### Decision 2: Use stable shared-MIME types only

The curated contract uses MIME types present in the target shared MIME database: `text/plain`, `application/x-zerosize`, `application/json`, `application/json5`, `application/toml`, `application/yaml`, and `text/markdown`.

Alternatives considered:

- Include JSONC as `application/jsonc` or `text/x-jsonc`. Rejected for this change because those types are not present as stable shared-MIME rows on the target desktop stack.
- Include Properties as `text/x-java-properties` or `text/x-properties`. Rejected for this change because `.properties` resolves to `text/plain` on the target desktop stack without a custom MIME definition.
- Add project-owned MIME definitions for JSONC and Properties. Deferred because that would turn a desktop-entry allowlist cleanup into a broader shared-MIME packaging change.

### Decision 3: Verify generated and runtime-visible metadata

Implementation must validate the generated `.desktop` file, refresh the relevant applications cache, and verify GLib's view with `gio mime` for all allowed MIME types. It must also verify that removed source MIME types are not present in generated/staged `MimeType` fields, while allowing GLib's inherited parent-type behavior to keep LushText visible for some `text/plain` subclasses outside the explicit GNOME Settings File Types list.

Alternatives considered:

- Inspect only `data/dev.cominotti.lushtext.desktop.in`. Rejected because stale exported desktop files or stale MIME caches can make source metadata correct while GNOME Settings remains wrong.
- Require LushText to become the default handler for the curated types. Rejected because installation and dev staging must not mutate user defaults.

## Risks / Trade-offs

- Removing source MIME registrations can make LushText disappear from GNOME Settings rows for Rust, Python, C, or headers -> Mitigation: this is intentional; users can still open those files explicitly, and LushText remains a general text editor at runtime.
- `gio mime` output can include user defaults or registrations from stale desktop entries -> Mitigation: acceptance checks should distinguish registered/recommended presence from default ownership, and should continue checking for same-ID non-Flatpak desktop shadows.
- Some source-like MIME types may still list LushText in `gio mime` because they inherit from `text/plain` -> Mitigation: treat the explicit desktop `MimeType` field as the File Types allowlist, and do not use inherited subtype association output as evidence that LushText explicitly advertises those source types.
- JSONC and Properties may be requested later -> Mitigation: document that those require a separate shared-MIME decision rather than silently adding unstable MIME strings now.

## Migration Plan

1. Update the desktop entry `MimeType` field to the exact curated allowlist.
2. Ensure development desktop-entry staging continues to derive from that template and refreshes the local applications database after staging and restore.
3. Update Flatpak identity and MIME verification scripts or fixtures so the allowlist is checked consistently.
4. Validate generated desktop metadata.
5. Refresh or rebuild the Flatpak/exported desktop entry and verify GLib registration for the curated MIME set.
6. Verify source-language MIME strings are absent from packaged and development desktop entries.

Rollback is a metadata-only revert: restore the previous `MimeType` field, refresh the same desktop MIME caches, and rerun the same GLib visibility checks.

## Open Questions

None for this proposal.
