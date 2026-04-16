## 1. Contract And Documentation

- [x] 1.1 Update `docs/next/session-time-travel.md` so the local-history browser is described as a large, preview-first viewer that stays adaptive and GNOME-HIG-friendly.
- [x] 1.2 Keep the local-history implementation notes aligned with the new parent-bounded sizing and preview-dominant layout contract.

## 2. Viewer-Scale Browser Layout

- [x] 2.1 Change the populated local-history dialog to derive a much larger default size from the current window while keeping it smaller than the parent window.
- [x] 2.2 Adjust the wide-layout split configuration so the snapshot list acts as a browse rail and the preview keeps the majority of the side-by-side width.
- [x] 2.3 Preserve the current narrow-window collapse flow and keep the no-snapshots dialog intentionally compact.

## 3. Verification

- [x] 3.1 Add widget-test coverage for the new wide-window local-history dialog scale and preview-dominant split behavior.
- [x] 3.2 Re-run the targeted local-history widget coverage to confirm the empty state, adaptive collapse flow, restore flow, and new geometry contract still behave correctly.
