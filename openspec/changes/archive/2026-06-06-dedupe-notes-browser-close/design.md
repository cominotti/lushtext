## Context

The populated `Browse Notes...` dialog is built as an `AdwDialog` whose child is an `AdwNavigationSplitView`. The sidebar page currently owns a small header with the dialog title and a Close/X button, while the preview page owns a second header with the selected note title, Back button, and another Close/X button. This fixed the earlier bug where the dialog lacked an obvious close affordance, especially when the split view collapses and only one page is visible.

The downside is that wide/unfolded layouts display both pages at the same time, so the user sees two equivalent Close/X controls for one dialog. GNOME HIG guidance points toward sparse header controls and a single window/dialog dismissal affordance in the surrounding chrome. The notes browser should keep the reliability of the current close path while making the dismissal affordance belong to the dialog shell rather than to each split-view page.

## Goals / Non-Goals

**Goals:**

- Present exactly one visible Close/X affordance for the populated notes browser when the sidebar and preview are both visible.
- Keep a visible Close/X affordance available when the split view collapses to either the sidebar page or the preview page.
- Keep the empty notes-browser state visibly dismissible.
- Keep `Escape` closing the dialog immediately after opening, regardless of whether focus is inside search, the sidebar, preview content, or a button.
- Preserve existing notes-browser behavior for search, sectioning, preview, Open, workspace scope, and open-tab supplemental rows.

**Non-Goals:**

- Redesigning the notes browser information architecture.
- Changing bookmark, document-note, workspace-note, or open-tab collection semantics.
- Introducing new persistence, sidecar, or migration behavior.
- Reworking unrelated dialogs that also use `build_dialog_close_button`.

## Decisions

1. Move the canonical Close/X to the notes-browser dialog shell.

   The populated browser should wrap the `AdwNavigationSplitView` in a vertical shell with a single top header containing the dialog title and Close/X button. The close control should be created once and wired to the owning `AdwDialog`. The sidebar page should no longer append its own close button, and the preview page should no longer append its own close button.

   Alternative considered: keep the sidebar close button and make the preview close button visible only while the split view is collapsed and the preview page is active. This is a smaller implementation, but it makes close availability depend on adaptive page state and keeps duplicate close ownership in the page builders. A shell-owned close is more coherent long term because there is one dialog and one dialog-level dismissal control.

2. Keep page-local navigation separate from dialog dismissal.

   The preview page's Back button should remain page-local navigation and should continue to appear only when it is meaningful in collapsed navigation. It should not be treated as a dismissal control and should not replace the dialog Close/X.

   Alternative considered: rely on Back plus Escape in collapsed preview mode. That would regress the earlier UX fix because users would lose an explicit visible way to close from the preview page.

3. Reuse the existing close helper and Escape controller.

   The existing `build_dialog_close_button` helper already supplies the standard close icon, tooltip, accessible label, and close callback. The shell header should reuse it. Existing `install_dialog_escape_close` behavior should remain installed on the dialog shell and focusable children that can consume key events.

   Alternative considered: switch to a new dialog type or Libadwaita header widget as part of this change. That is unnecessary for the behavioral bug and risks broad geometry churn in a modal that already has focused widget coverage.

4. Treat the empty state as a single-page shell.

   The empty notes browser already has one visible Close/X. It can keep its current structure unless implementation naturally shares the new shell header. The requirement is that it remains visibly dismissible without gaining any duplicate close controls.

## Risks / Trade-offs

- [Risk] Adding a shell header above the split view could create a double-title feel if the sidebar still starts with a `Notes` title. -> Mitigation: remove the sidebar page-local title/header in the populated browser, or convert it into shell-owned title chrome.
- [Risk] Collapsed preview mode could lose an obvious close path if the shell header is hidden by navigation content. -> Mitigation: keep the shell header outside the `AdwNavigationSplitView` so it remains visible above both collapsed pages.
- [Risk] Tests may pass in only one adaptive state and miss duplicate controls in the other. -> Mitigation: add widget coverage for unfolded close-control count and collapsed sidebar/preview dismissal paths.
- [Risk] The previous close-affordance change required closing from both visible pages. -> Mitigation: interpret that requirement as reachability from each visible state, not as two simultaneously visible buttons. The shell close satisfies both pages because it remains visible regardless of which page is active.
