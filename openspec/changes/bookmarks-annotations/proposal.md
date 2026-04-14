## Why

LushText does not yet give users a non-destructive way to mark important spots in a file or leave private notes for later. People fall back to temporary source edits, external notes, or memory, which breaks down for read-only files, shared repositories, and long editing sessions.

## What Changes

- Add persistent per-file line bookmarks that users can toggle, label, and navigate without modifying the file content.
- Add persistent sidecar annotations anchored to line ranges so users can keep short notes outside the original file.
- Surface bookmarks and annotations through editor gutter affordances, jump/list workflows, and lightweight editing UI.
- Preserve bookmark and annotation data across sessions and keep it outside version-controlled source files by storing it under the app data directory.
- Support exporting annotations into a shareable markdown summary for review or handoff workflows.

## Capabilities

### New Capabilities
- `line-bookmarks`: Persistent per-file bookmarks with gutter visibility, optional labels, session-safe storage, and in-file navigation.
- `sidecar-annotations`: Persistent line-range annotations stored outside the source file, with lightweight editing, browsing, and export support.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/model`, `services`, `ui/editor_page`, `ui/window`, command/search surfaces, and related tests.
- Affected systems: GtkSourceView gutter integration, sidecar JSON persistence in the user data directory, and new GSettings/preferences for bookmark visibility.
- Dependencies and APIs: builds on existing JSON persistence patterns and GtkSourceView mark APIs; may add new internal models/services for bookmark and annotation lifecycle management.
