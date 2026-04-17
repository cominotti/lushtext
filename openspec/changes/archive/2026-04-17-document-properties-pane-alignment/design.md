## Context

LushText currently has the shell pieces needed for a GNOME-style document-properties workflow, but the responsibility split is muddy. The main window already uses a left `workspace_split_view` and a right `properties_split_view`, yet the bottom bar still owns overlapping current-document metadata and actions such as encoding, line endings, file size, EditorConfig state, and file health. At the same time, the right properties pane shows another subset of current-document metadata and editor controls.

That overlap creates three problems:

- it weakens the distinction between a navigation sidebar, a utility pane, and a status surface
- it forces encoding and file-health workflows to straddle two surfaces
- it leaves compact-width behavior underspecified when the workspace sidebar and document properties both want scarce secondary-surface space

The change also needs to fit the current repo constraints:

- preserve the outer workspace-sidebar architecture and its adaptive width policy
- align with current GNOME HIG guidance for utility panes and header-bar controls
- match current GNOME Text Editor behavior closely enough that the info button, shortcut, and compact presentation feel familiar
- preserve a richer, Builder-like bottom bar where that is a better fit for LushText's editor-facing workflow
- avoid turning the narrow properties surface into a catch-all browsing area; browse-heavy workflows still belong in dedicated dialogs

## Goals / Non-Goals

**Goals:**

- Create a clear, non-duplicative split between the bottom bar, the document properties surface, and Preferences.
- Move the document-properties toggle to the top-right header-bar position used by GNOME Text Editor and assign `F9` to that surface.
- Make the document-properties surface adapt as a right pane on spacious windows and as a bottom sheet on compact windows.
- Ensure compact layouts never leave the workspace sidebar and document-properties surface visible together.
- Preserve the existing safety model for encoding, line-ending, and file-health flows while keeping encoding and line-ending actions in the bottom bar.
- Keep the workspace sidebar width policy and outer shell math intact wherever possible.

**Non-Goals:**

- Redesign the workspace sidebar width presets or its existing adaptive clamping policy.
- Turn the properties surface into a general-purpose browser for bookmarks, annotations, local history, or other browse-heavy tools.
- Introduce a document type or language picker in the document properties surface.
- Rework unrelated status-bar features such as future cursor-position or search-progress indicators.
- Introduce a new persistence model for pane visibility when the existing settings keys can be reused.

## Decisions

### 1. Treat document properties as one adaptive surface, not two unrelated widgets

The implementation should keep the outer workspace `AdwOverlaySplitView`, but replace the inner "properties are always a split view" assumption with a dedicated adaptive host:

- **Spacious layout:** the document-properties surface is a right-side utility pane.
- **Compact layout:** the same surface becomes a bottom sheet opened by the same header-bar toggle.

The cleanest technical shape is an inner `AdwMultiLayoutView` with:

- a wide layout using `AdwOverlaySplitView` for the right properties pane
- a compact layout using `AdwBottomSheet` for the same properties content

This follows the current GNOME Text Editor model closely while limiting structural churn to the inner document-properties relationship. The outer workspace sidebar can remain on the existing split-view architecture and keep its width policy unchanged.

**Alternatives considered**

- Keep the current inner `AdwOverlaySplitView` and let it overlay at narrow widths.
  Rejected because it diverges from current GNOME Text Editor behavior and still leaves two different compact secondary-surface models competing in one window.
- Replace both left and right shell layers with a single new adaptive architecture.
  Rejected because it would entangle this change with unrelated workspace-sidebar geometry work that is already stable on `main`.

### 2. Split responsibilities instead of moving all current-document state into one surface

LushText should use a hybrid ownership model:

- **Bottom bar:** high-frequency, glanceable editor state
  - encoding
  - line endings
  - EditorConfig badge when per-file overrides are active
  - workspace toggle
  - transient feedback or notifications
- **Document properties surface:** slower, inspectable document information
  - path or location
  - file size
  - formatting source
  - file-health details
  - document statistics
  - other document-scoped inspection rows
- **Preferences:** app-wide editor defaults
  - `Use EditorConfig`
  - word wrap
  - line numbers
  - current-line highlight
  - default tabs/spaces behavior

This gives the UI a clearer mental model without forcing LushText into GNOME Text Editor's very light status bar. It also matches Builder's use of the bottom bar as a quick editor telemetry strip while still letting LushText adopt Text Editor's better document-properties toggle and compact behavior.

The `EditorConfig` split is intentional rather than accidental duplication:

- the bottom-bar badge answers "is this file currently overridden?"
- the formatting-source row answers "why is this file behaving this way?"
- the Preferences toggle answers "is this feature enabled globally?"

**Alternatives considered**

- Move all current-document state into the document properties surface.
  Rejected because it overcorrects toward GNOME Text Editor and gives up a bottom-bar pattern that appears to fit development-oriented editing well.
- Keep the current overlap and just polish labels.
  Rejected because the real problem is duplicate ownership and contradictory surface roles.
- Move everything into the bottom bar and shrink the right properties surface.
  Rejected because slower document inspection details are a better fit for a utility pane than for a dense bottom strip.

### 2b. Keep the current dynamic properties breakpoint guard

The switch from right-side pane to bottom sheet should keep using the existing editor-width guard logic rather than a new fixed threshold. The current guard already:

- protects the minimum comfortable editor width
- accounts for whether the workspace sidebar is actually consuming width
- tracks the active workspace preset's effective width rather than a raw hint fraction

That makes it a better fit than a single hardcoded width such as `1100sp` or `1200sp`. With the current shell math, the representative thresholds are already:

- `912sp` when no workspace sidebar width is being consumed
- about `1350sp` for the default `Comfy` workspace preset while the workspace sidebar is consuming width

So the design decision is not "pick a new breakpoint", but "reuse the current breakpoint function and reinterpret its compact state as bottom-sheet mode instead of right-overlay mode."

**Alternatives considered**

- Introduce a new fixed pane-to-sheet threshold.
  Rejected because it would throw away the editor-width reasoning the current shell already encodes and would behave poorly across workspace width presets.
- Tie the sheet switch directly to the outer workspace collapse threshold.
  Rejected because the right-side surface becomes cramped well before the workspace sidebar fully collapses.

### 3. Reuse existing internal action and settings identities where possible

The user-visible contract will change, but the implementation should prefer continuity for internal wiring where that reduces migration risk. Existing identifiers such as `win.toggle-properties` and `properties-sidebar-visible` can be retained as the stored "document properties requested visibility" state even though the compact presentation is a bottom sheet rather than a side pane.

What changes is the rendering layer:

- stored visibility represents the user’s desired document-properties state
- the adaptive layout decides whether that state is rendered as a right pane or a bottom sheet

This keeps migration shallow and avoids spreading renamed action IDs and settings keys across unrelated code and tests.

**Alternatives considered**

- Rename the action and settings to introduce a new "document-properties-surface" vocabulary everywhere.
  Rejected because it adds migration noise without user-visible benefit.

### 4. Separate explicit pane intent from compact-layout suppression

Compact layouts need arbitration because only one secondary surface can remain visible at a time. The right behavior is:

- if document properties are opened in compact mode, the workspace sidebar closes
- if the workspace sidebar is opened in compact mode while document properties are visible, document properties close
- temporary compact suppression does not itself overwrite the remembered wide-layout visibility intent

This requires a distinction between:

- **requested visibility**: what the user explicitly left open or closed
- **rendered visibility**: what the compact layout can actually show at one time

Without that separation, a simple resize would silently rewrite user intent and produce surprising restoration when widening again.

**Alternatives considered**

- Persist the compact winner directly into the existing visibility settings.
  Rejected because resizing alone would behave like an explicit user close.
- Always prioritize the workspace sidebar in compact mode.
  Rejected because the user explicitly asked which surface should yield, and document properties are the GNOME Text Editor-like subordinate surface being directly requested from the header bar.

### 5. Keep quick encoding controls in the bottom bar and move slower health inspection into the document-properties surface

Encoding and line-ending controls are fast, repetitive, and often used while the user's attention stays in the editor. They fit the bottom bar better than the document-properties surface. File-health inspection is slower and more explanatory, so it fits the document-properties surface better.

This produces a more stable split:

- bottom bar keeps the quick state labels and lightweight pickers
- document properties surface owns file-health details and slower inspection rows
- broad encoding choosers and lossy-save confirmations remain modal
- persistent warnings remain document-scoped outside the pane when needed

**Alternatives considered**

- Move encoding and line endings into the document-properties surface too.
  Rejected because those are exactly the kind of high-frequency controls that a Builder-like bottom bar handles well.
- Keep file-health details in the bottom bar beside encoding and line endings.
  Rejected because health details are slower and more inspectable, and they would crowd the strip more quickly than quick state labels do.

### 6. Leave document type or language out of this delta

LushText already performs automatic syntax detection from the file path, but it does not yet expose a real user-facing document type workflow. Adding a `Document Type` row now would force one of two awkward outcomes:

- a read-only row that adds little value while expanding scope
- an editable row that turns this change into a separate language-picker feature, with questions about persistence, large-file syntax-disable behavior, untitled documents, and fallback semantics

That makes document type or language a poor fit for this already cross-cutting shell change. If it proves valuable later, a follow-up delta can choose between a read-only row and a full manual-language workflow explicitly.

**Alternatives considered**

- Add a read-only language row now because auto-detection already exists.
  Rejected because it adds noise to the properties surface before the core surface ownership is even stable.
- Add a language picker now because GNOME Builder exposes one.
  Rejected because Builder's picker is a distinct editor feature, not just a presentation tweak, and it would materially widen this change's scope.

## Risks / Trade-offs

- **[Adaptive host complexity]** -> Restrict the new layout indirection to the inner document-properties relationship and keep the outer workspace split untouched.
- **[Shortcut reassignment surprise]** -> Update tooltips and the shortcuts dialog so `F9` clearly maps to document properties after the change, while the workspace toggle remains visible in the bottom bar.
- **[Properties surface crowding]** -> Keep the surface scoped to current-document information and immediate actions; defer browse-heavy tools to dialogs.
- **[Bottom-bar crowding]** -> Keep this delta focused on encoding and line endings only; leave cursor position and indentation for a later change instead of letting this scope sprawl.
- **[Language scope creep]** -> Keep document type or language out of this delta entirely so the shell refactor does not accidentally become a manual syntax-selection feature.
- **[Visibility restoration bugs during resize]** -> Use explicit requested-versus-rendered visibility state and cover resize transitions with widget tests.
- **[Spec drift in encoding flows]** -> Update `encoding-toolkit` in the same change so implementation does not inherit conflicting status-bar language.

## Migration Plan

1. Update the OpenSpec contract for the new document-properties surface and the modified encoding-toolkit behavior.
2. Restructure the inner document-properties host to support wide-pane and compact-sheet layouts while preserving the outer workspace split and width policy.
3. Move the document-properties toggle into the header bar, reassign `F9`, and remove the bottom-bar properties toggle.
4. Re-split ownership so the bottom bar keeps encoding and line endings, the document-properties surface gains slower document-inspection details and file-health details, and app-wide editor defaults return to Preferences-only ownership.
5. Add verification for wide layout, compact layout, resize transitions, and compact mutual exclusion.

Rollback remains straightforward because the existing `properties-sidebar-visible` and `win.toggle-properties` identities can continue to exist throughout the migration; reverting the layout host and template wiring would restore the current shell model without a settings migration.

## Open Questions

None for this proposal. Cursor position, indentation, and any future document type or language row are intentionally deferred to later deltas once the core surface split and compact behavior are stable.
