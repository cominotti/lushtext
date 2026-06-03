## 1. Status-Bar Message Area Structure

- [ ] 1.1 Wrap `message_label` in a full-width `message_area_box` template child that owns the message lane between the workspace toggle and metadata controls.
- [ ] 1.2 Move message padding/spacing inside the new message-area wrapper so the flash background covers empty horizontal message space while text alignment stays unchanged.
- [ ] 1.3 Add status-bar widget state for pulse generation and class alternation, keeping the state local to `LushtextStatusBar`.
- [ ] 1.4 Add a status-bar pulse API that applies severity-specific pulse classes to the message-area wrapper and removes stale pulse classes with a generation guard.

## 2. Notification Bridge Behavior

- [ ] 2.1 Trigger the message-area pulse after transient status messages are published and confirmed as the visible status-bar view.
- [ ] 2.2 Trigger the message-area pulse after progress updates only when the updated progress message is the visible status-bar view.
- [ ] 2.3 Keep expiry sweeps, resolves, generic renders, and progress heartbeats from starting a pulse.
- [ ] 2.4 Ensure rapid repeated identical visible messages restart the pulse without changing the displayed notification text.

## 3. CSS and Theme Treatment

- [ ] 3.1 Add base CSS for the status-bar message area without changing the existing bottom-bar height or metadata layout.
- [ ] 3.2 Add paired info, warning, and error pulse classes/keyframes that briefly color the full message area and then fade back to the steady status bar surface.
- [ ] 3.3 Scope pulse-time foreground contrast rules to the message label inside the message area so text stays readable in light and dark themes.
- [ ] 3.4 Confirm the workspace toggle and document metadata controls are outside the pulse selectors.

## 4. Tests

- [ ] 4.1 Add widget or unit coverage proving repeated identical visible notifications restart the pulse state.
- [ ] 4.2 Add coverage proving pulse classes apply to the full message-area wrapper rather than only to `message_label`.
- [ ] 4.3 Add coverage proving heartbeat, sweep, resolve, and hidden-under-transient progress updates do not trigger a pulse.
- [ ] 4.4 Add coverage for info, warning, and error severity pulse class selection.

## 5. Validation and Documentation

- [ ] 5.1 Update repo guidance if the permanent status-bar structure or behavior notes need to mention the message-area pulse contract.
- [ ] 5.2 Run `openspec validate flash-status-message-area --strict`.
- [ ] 5.3 Run the relevant Rust and widget validation lanes for status-bar UI changes.
- [ ] 5.4 Manually verify in a live app session that repeated Save actions flash the full message area, warning/error messages use their severity treatment, and unrelated status-bar controls do not flash.
