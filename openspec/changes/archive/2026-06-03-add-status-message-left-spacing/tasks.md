## 1. Message-Area Layout

- [x] 1.1 Add a small start margin to the `message_area_box` template child so the flash background begins after a visual gap from the workspace-sidebar toggle.
- [x] 1.2 Preserve the existing `message_label` internal offset, ellipsizing, and hexpand behavior inside the message area.
- [x] 1.3 Confirm the spacing change does not alter status-bar height or move the document metadata controls into the pulse area.

## 2. Styling Contract

- [x] 2.1 Keep info, warning, and error pulse selectors scoped to `.status-message-area` so the new gap and workspace toggle do not flash.
- [x] 2.2 Avoid adding global status-bar spacing that would affect metadata controls or unrelated status-bar children.

## 3. Tests

- [x] 3.1 Add or update widget coverage proving `message_area_box` has a small start margin in the expected "few pixels" range.
- [x] 3.2 Add or update coverage proving the message label remains a child of the message area and keeps its compact internal offset.
- [x] 3.3 Add or update coverage proving pulse classes still apply to `message_area_box`, not the workspace toggle or metadata controls.

## 4. Validation

- [x] 4.1 Run `openspec validate add-status-message-left-spacing --strict`.
- [x] 4.2 Run the relevant status-bar widget validation lane.
- [x] 4.3 Manually verify or screenshot-check that repeated Save leaves a small non-flashing gap between the workspace toggle and the blue message-area flash.
