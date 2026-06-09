## Context

`LushtextInfoBar` renders editor-scoped warning and error notifications above the editor content. The current template keeps the message and action group inside an `AdwWrapBox`, and CSS gives the alert surface `8px 12px` padding plus a bottom border. In the restored-document warning shown alongside the workspace sidebar and document-properties pane, that vertical padding makes the yellow surface land slightly below nearby panel boundaries.

The desired refinement is intentionally small: make the alert just a little shorter while keeping it visually balanced. The alert must continue to read as an editor-scoped recovery surface, not a dense status strip.

## Goals / Non-Goals

**Goals:**

- Reduce the default editor inline-alert vertical footprint while keeping equal top and bottom padding.
- Preserve the current `AdwWrapBox` message/action layout, grouped action row, button contrast, warning/error colors, accessible alert semantics, and dismiss behavior.
- Keep restored-document warnings, retryable error alerts, informational warnings, and long/narrow alert content readable.
- Verify the change through focused widget/CSS checks and at least one real visual pass against the restored-document warning layout.

**Non-Goals:**

- Do not change alert wording, notification payloads, callback routing, draft/session recovery semantics, or status-bar notifications.
- Do not redesign the document-properties panel, workspace sidebar, header/tab chrome, or editor text margins. A narrow top-margin calibration of the existing document-properties content is allowed only to align the restored-warning screenshot without changing panel structure or row spacing.
- Do not make padding asymmetric for a one-off screenshot alignment.
- Do not reduce button internal padding or font sizes, since that would risk cramped controls and weaker accessibility.

## Decisions

### Tighten the alert surface padding evenly

The implementation should reduce `.editor-inline-alert` vertical padding from the current `8px` rhythm to a smaller balanced value. Pixel review showed `6px 12px` keeps the alert border aligned with the workspace separator. The document-properties panel content is nudged down by 3px so the Location card joins that same horizontal line rather than forcing the alert to become too short.

```css
.editor-inline-alert {
  padding: 6px 12px;
  border-bottom: 1px solid @borders;
}
```

This changes the alert shell's breathing room and pairs it with a tiny document-properties content offset. It leaves the horizontal inset, button styling, text styles, and border intact, so the alert still reads as the same warning/error component.

Rejected alternative: use asymmetric padding such as `8px 12px 4px`. That could line up one screenshot more exactly, but it would make the alert feel top-heavy and less reusable across warning, error, and wrapped layouts.

### Keep the layout container, with a tiny action-row optical nudge

The `AdwWrapBox` child spacing, line spacing, action button grouping, and label wrapping should remain unchanged unless verification proves they are involved in the vertical mismatch. Pixel review showed the action buttons sat about one pixel high inside the restored-draft alert after the outer edges were aligned, so the action row receives a 1px top margin while preserving the same grouped child, action order, and wrapping behavior.

Rejected alternative: shrink the action buttons or alter `line-spacing`. That would either make controls feel cramped or change the wrapped layout, which is outside this polish request.

### Verify state extremes instead of tuning only the screenshot

The implementation should verify the no-alert state, representative restored-document warnings, retryable error alerts, informational warnings with only dismiss, long/awkward text or action labels, and constrained editor widths. The important visible contract is that alerts are a little shorter in normal one-line layouts while text, workflow actions, and dismiss stay readable and reachable.

## Risks / Trade-offs

- [Risk] A smaller shell padding could make warning/error alerts feel cramped on some themes. -> Mitigation: keep the reduction modest and balanced, and inspect both warning and error variants.
- [Risk] A CSS-only test could prove the value but miss the human alignment issue. -> Mitigation: pair focused assertions with a real `make run` or headless visual pass of a restored-document warning with side panels visible.
- [Risk] Narrow wrapped alerts may feel denser after the padding reduction. -> Mitigation: keep `AdwWrapBox` line spacing unchanged and verify positive allocations/readability at constrained widths.
- [Risk] Changing adjacent panel spacing could disturb broader shell rhythm. -> Mitigation: keep the document-properties adjustment to the top content margin only, preserving panel structure, row spacing, and right-pane width behavior.

## Migration Plan

1. Update the scoped `.editor-inline-alert` CSS padding to the balanced compact value.
2. Add or update focused widget/CSS coverage that proves the alert uses balanced compact padding and keeps scoped selectors.
3. Re-run the existing inline-alert widget tests for wide/narrow layouts and warning/error/no-action states.
4. Confirm the standalone `gtk4-builder-tool` limitation for `AdwWrapBox` has not changed, then rely on the widget harness for template validation because it initializes Libadwaita before constructing `LushtextInfoBar`.
5. Capture or manually inspect a restored-document warning with workspace sidebar and document properties visible to confirm the bottom edge aligns cleanly without looking cramped.

Rollback is local: restore the previous `.editor-inline-alert` padding if visual inspection shows the compact rhythm makes warning or error alerts feel too dense.

## Open Questions

None.
