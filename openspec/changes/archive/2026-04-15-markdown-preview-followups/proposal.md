## Why

LushText's native Markdown preview now covers the highest-value GitHub-flavored basics, but common README-style documents still lose meaning when links cannot be opened, table-cell links collapse to plain text, local images disappear, and nested lists lose hierarchy. These are the next high-value follow-ups that still fit the project's GTK-native, simple-to-maintain renderer strategy.

## What Changes

- Add native link activation for supported Markdown links in the read-only preview so users can open them externally without leaving the GTK rendering path.
- Extend rendered table cells so supported links keep link styling and activation instead of degrading to plain text.
- Render local Markdown images, including workspace-relative paths, as read-only native image blocks within the preview flow.
- Tighten nested ordered and unordered list rendering so deeper hierarchies and mixed list structures remain readable.
- Add deterministic Markdown preview coverage for interactive links, table-cell links, local image rendering and fallback states, and nested-list fidelity.

## Capabilities

### New Capabilities
- `markdown-preview-links`: Render supported Markdown links as activatable preview content that opens externally from the read-only native preview.
- `markdown-preview-local-images`: Render local Markdown image syntax and workspace-relative image paths as native preview image blocks with explicit fallback behavior.
- `markdown-preview-nested-lists`: Preserve readable hierarchy for nested ordered and unordered Markdown lists in the native preview.

### Modified Capabilities
- `markdown-preview-tables`: Extend table-cell rendering requirements so supported inline links remain recognizable and activatable inside rendered tables.

## Impact

- Affected code: `crates/lushtext-core/src/ui/markdown_preview`, preview styling/resources as needed, and Markdown preview tests under `crates/lushtext/tests/widget/markdown_preview.rs`.
- Affected systems: pulldown-cmark event handling for links, images, and nested list structure; GTK-native preview rendering paths for `GtkTextBuffer`, anchored widgets, and table-cell widgets.
- Dependencies and APIs: no HTML or WebKit path is introduced; the change is expected to build on existing GTK/GIO primitives for external URI launching and native image loading.
