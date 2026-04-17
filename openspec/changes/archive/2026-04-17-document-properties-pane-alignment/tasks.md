## 1. Adaptive Shell

- [x] 1.1 Replace the inner always-on `properties_split_view` assumption with an adaptive document-properties host that uses a right-side pane on spacious layouts and a bottom sheet on compact layouts while preserving the outer workspace split and reusing the existing dynamic properties breakpoint guard.
- [x] 1.2 Move the document-properties toggle into the header bar with `info-outline-symbolic`, bind it to the adaptive document-properties surface, and reassign `F9` from the workspace sidebar to document properties.
- [x] 1.3 Introduce requested-versus-rendered visibility handling so compact layouts can temporarily suppress one secondary surface without losing the user’s explicit wide-layout visibility intent.

## 2. Surface Ownership Cleanup

- [x] 2.1 Expand the document properties surface around slower document-inspection details such as path or location, file size, formatting source, statistics, and file-health details.
- [x] 2.2 Remove the bottom-bar document-properties toggle and duplicated slow document-detail rows, while keeping encoding and line-ending controls plus the terse `EditorConfig` badge in the bottom bar.
- [x] 2.3 Remove app-wide editor-default controls from the document properties surface so Preferences remains the home for global editor defaults.
- [x] 2.4 Ensure untitled and empty-window states keep explicit, non-stale document-properties copy when the active document cannot provide file-backed metadata.
- [x] 2.5 Keep `EditorConfig` split by role: bottom-bar badge for glanceable state, formatting-source explanation in document properties, and the global toggle only in Preferences.
- [x] 2.6 Do not add a `Document Type` row or language picker in this delta.

## 3. Encoding Toolkit Migration

- [x] 3.1 Keep encoding and line-ending entry points in the bottom bar while moving file-health inspection into the document-properties surface without regressing active-document refresh behavior.
- [x] 3.2 Preserve the existing modal reopen/save confirmation flows and persistent warnings while updating launch points, tooltips, and user-visible labels to match the new hybrid ownership model.
- [x] 3.3 Update any encoding-related status, shortcut, or help surfaces that still describe the old overlapping model.

## 4. Compact Coordination

- [x] 4.1 Implement compact-layout mutual exclusion so opening document properties closes the workspace sidebar and opening the workspace sidebar closes compact document properties.
- [x] 4.2 Ensure resize transitions between spacious and compact layouts restore pane visibility according to the most recent explicit user choices instead of treating temporary compact suppression as a permanent close.

## 5. Verification and Follow-Through

- [x] 5.1 Add or update widget coverage for the wide right-pane presentation, compact bottom-sheet presentation, header-bar toggle state, and `F9` shortcut behavior.
- [x] 5.2 Add or update widget coverage for compact mutual exclusion and restoration after widening back to a spacious layout.
- [x] 5.3 Add or update widget coverage showing the hybrid ownership split: encoding and line endings stay in the bottom bar while slower document details and file-health details live in the document-properties surface.
- [x] 5.4 Refresh repo documentation or notes that still describe the old bottom-bar overlap or the pre-alignment properties-pane contract.
