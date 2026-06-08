## Context

`Browse Notes...` now matters as a window-level entry point: users can open it even with no tabs and no workspaces. The empty path currently builds a compact `AdwDialog` with a `No notes yet` `AdwStatusPage`, but it also allows the dialog to follow the child content size. In practice, that can collapse the modal into a narrow column where the icon, title, and description wrap awkwardly.

The populated Notes browser already uses fixed browser dimensions with `follows_content_size(false)`, because the browser shell needs room for list, preview, and action content. The empty state does not need the full populated browser shell, but it does need a stable readable content area.

## Goals / Non-Goals

**Goals:**

- Keep the empty Notes browser compact but legible when opened from the header menu, command palette, or action surface.
- Ensure the empty Notes browser fits its icon, title, description, margins, and close affordance without showing a vertical scrollbar.
- Ensure the dialog's actual rendered allocation honors the intended empty-state size instead of shrinking to the status page's natural width.
- Preserve current empty-state semantics: no fake sidebar rows, no created notes, no created workspaces, and the same close/Escape behavior.
- Add widget coverage that would fail for the narrow-column screenshot, not only for missing labels.
- Check whether the adjacent Local History empty dialog shares the same sizing failure mode.

**Non-Goals:**

- Do not redesign the populated Notes browser.
- Do not add a Notes onboarding flow, creation buttons, or new note commands.
- Do not change note storage, note search, bookmark indexing, or workspace scoping.
- Do not move `Browse Notes...` into another menu surface.

## Decisions

### Keep a dedicated compact empty dialog

The empty state should remain a small browser-style modal rather than instantiating the populated split-view browser with an empty sidebar. This keeps the first-run empty state visually quiet and avoids showing an inert search/sidebar surface when there are no notes to browse.

Alternative considered: reuse the populated Notes browser shell with an empty result list. That would make geometry consistent with populated browsing, but it would look unnecessarily heavy for the no-data state and could imply that search/filtering is useful before any notes exist.

### Make the dialog honor its target size

The empty Notes dialog should use a stable content size contract. In practice, that means treating `content_width` and `content_height` as the modal target and preventing child natural-size measurement from collapsing the shell. The target height must also be tall enough for the normal `AdwStatusPage` plus header and margins so the empty state does not need a vertical scrollbar. The content box and status page should expand inside that target so text gets a readable line length.

Alternative considered: only setting more label wrapping or CSS on the status page. That treats the symptom but leaves the modal shell governed by child natural size, so a theme, font, translation, or future content tweak could collapse it again.

### Test rendered allocation, not just properties

Widget coverage should inspect the visible `AdwDialog` after layout settles and assert meaningful allocation constraints. Checking `dialog.content_width()` alone is too weak: the current code can report the intended content width while still rendering as a narrow modal when `follows_content_size` lets child measurement win.

The strongest focused assertions are:

- the empty Notes dialog does not follow child content size
- the settled dialog allocation is wide and tall enough for the status page
- no internal scrolled surface has vertical overflow in the empty state
- the `No notes yet` label still appears
- no `AdwSidebar` is materialized for the empty no-workspace state

### Audit Local History without broadening the Notes fix by default

Local History has a nearby empty-dialog pattern. The implementation should inspect it during apply. If it reproduces the same collapse, align it with the same stable empty-dialog sizing approach; if not, leave it alone and keep this change scoped to Notes.

## Risks / Trade-offs

- [Risk] A fixed compact empty-state size could feel slightly larger than the minimum content needs. -> Mitigation: keep the target around the existing `560x360` intent rather than using the full populated `980x700` browser size.
- [Risk] Tests that assert exact rendered pixels can be brittle across GTK themes. -> Mitigation: assert lower bounds and sizing contract properties instead of exact outer dimensions.
- [Risk] Local History may share the same code smell. -> Mitigation: include an explicit audit task so the pattern is not forgotten, but keep implementation changes limited to confirmed affected surfaces.
