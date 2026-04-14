## Why

LushText handles large files and already offers bookmarks, search, and other navigation aids, but users still lack a quick spatial overview of where they are inside a document. A minimap can turn that missing overview into a fast navigation surface and make clustered search hits, bookmarks, modified regions, and other important areas visible at a glance.

## What Changes

- Add a toggleable editor minimap on the right edge of each editor page using the existing GtkSourceView stack wherever possible.
- Show meaningful region markers in the minimap for navigation-relevant document state such as search matches, bookmarks, modified-since-save ranges, and long-line warnings when those signals are available.
- Support direct minimap interaction for jump-to-position behavior, viewport awareness, and auto-hide or disable rules when the minimap would add little value or unacceptable cost.
- Add settings, actions, and focused tests for minimap visibility, behavior, and large-file guardrails.

## Capabilities

### New Capabilities
- `editor-minimap`: Provide a toggleable document minimap with semantic region markers and direct navigation behavior inside editor pages.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/editor_page`, related window action and settings wiring, and any shared helpers needed for marker projection or large-file gating.
- Affected systems: GtkSourceView editor overlays, GSettings schema/defaults, keyboard shortcut wiring, and document-state signals that feed minimap markers.
- Dependencies and APIs: expected to rely first on built-in `GtkSourceMap`; no new external dependency is expected unless GTK limitations force a later custom widget path.
