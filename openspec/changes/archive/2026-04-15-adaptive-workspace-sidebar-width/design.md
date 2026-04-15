## Context

LushText currently exposes workspace sidebar sizing through a fixed footer row in the sidebar itself and represents `Small`, `Comfy`, and `Large` as exact full-window fractions (`0.2`, `0.3`, `0.4`). The window shell then locks the left `AdwOverlaySplitView` to that exact computed width on every resize by setting matching minimum and maximum sidebar widths.

That architecture is already deterministic and GTK-native enough for the current shell, but the policy being enforced is no longer a good fit for large displays. On wide and ultrawide windows, the selected preset continues to grow indefinitely because it is tied directly to total window width. At the same time, the control lives in sidebar chrome instead of the Preferences surface that already owns other workspace-level settings.

This change crosses the preferences UI, sidebar template, split-view width math, and widget-test contracts. It also affects the right properties-pane breakpoint because that breakpoint currently depends on the left preset's effective width.

## Goals / Non-Goals

**Goals:**
- Move workspace sidebar width selection into `Preferences > Workspace` using an Adwaita-native single-choice control.
- Keep the existing three presets (`Small`, `Comfy`, `Large`) so the feature remains easy to understand.
- Make each preset feel comfortable on large displays by capping how wide the workspace sidebar can grow.
- Preserve the current deterministic, non-draggable shell behavior so pane sizing stays reliable and testable.
- Keep right-pane fraction and breakpoint calculations aligned with the workspace sidebar's effective on-screen width.

**Non-Goals:**
- Reintroducing arbitrary drag-resizable workspace widths.
- Adding a character-count-based live sizing algorithm.
- Introducing new presets, custom numeric widths, or per-monitor saved widths.
- Changing workspace tree overflow behavior, drill-down behavior, or no-horizontal-scrollbar rules.

## Decisions

### 1. Move the control to Preferences and remove it from sidebar chrome

The width preset selector will move from the fixed sidebar footer to `Preferences > Workspace` as an `AdwComboRow` with `Small`, `Comfy`, and `Large`.

Why:
- It matches the role of the setting: this is a global layout preference, not an in-context action for the current workspace section.
- It reduces persistent chrome in the sidebar and keeps the tree area visually simpler.
- It fits the existing Preferences structure, which already has a Workspace page.

Alternatives considered:
- Keep the footer buttons in place: rejected because the control permanently occupies sidebar space and continues to emphasize a low-frequency preference.
- Duplicate the control in both places: rejected because duplicated state surfaces are harder to maintain and add unnecessary visual noise.

### 2. Keep the existing three presets, but redefine each as an adaptive width policy

Each preset will keep its current fraction hint while gaining preset-specific width bounds in `sp`:

- `Small`: hint `20%`, minimum `220sp`, maximum `280sp`
- `Comfy`: hint `30%`, minimum `280sp`, maximum `360sp`
- `Large`: hint `40%`, minimum `340sp`, maximum `440sp`

Runtime width policy:

`target_width_sp = clamp(window_width_sp * hint_fraction, min_width_sp, max_width_sp)`

Why:
- It keeps the familiar mental model of three named sizes.
- At the default `1200sp` window width, `Comfy` remains `360sp`, so the default feel stays close to today's baseline.
- On wide windows, each preset stops expanding once it reaches a comfortable upper bound.

Alternatives considered:
- Fixed `sp` widths only: viable, but the hybrid policy feels a little more natural on mid-width desktop windows.
- Character-count sizing using rendered monospace metrics: rejected because indentation, icons, paddings, font overrides, and prior overflow-measurement drift make it more fragile than the user-visible benefit justifies.

### 3. Continue using deterministic split-view locking

The window shell will continue to enforce a single computed workspace sidebar width by locking `min_sidebar_width` and `max_sidebar_width` to the same target and keeping the sidebar widget aligned to that value.

Why:
- This matches the current non-draggable shell contract.
- It keeps width changes predictable and easy to test.
- It avoids reopening the older geometry problems that motivated the move away from arbitrary dragging.

Alternatives considered:
- Let the split view remain freely draggable after selecting a preset: rejected because it turns presets into weak suggestions and complicates persistence and breakpoint behavior.

### 4. Treat the selected preset as the source of truth and compute the effective fraction at runtime

The selected preset remains the persisted choice. The shell will derive the effective on-screen width from the adaptive policy and then compute the effective split-view fraction from that width for the current window size.

Why:
- The adaptive policy means the visible width is no longer always equal to the preset's hint fraction.
- The right properties pane and its breakpoint must react to the actual left width being consumed, not the unclamped hint.

Alternatives considered:
- Keep using the unclamped hint fraction for downstream math: rejected because it would make the properties-pane guard too conservative on ultrawide windows.

### 5. Reuse the existing nearest-preset snapping behavior for old values

Existing stored preset values can keep snapping to the nearest named preset. No user-facing migration step is required.

Why:
- The app already resolves arbitrary stored fractions to one of the three presets.
- This keeps the rollout simple and avoids adding a new migration-only setting if the current backing value remains sufficient for preset identity.

Alternatives considered:
- Add a brand-new preset key and migrate all stored values: rejected as unnecessary complexity unless implementation reveals a strong reason to separate the stored preset from the current backing value.

## Risks / Trade-offs

- [Risk] The clamp ranges could make adjacent presets feel too similar on medium-width windows. → Mitigation: keep the ranges intentionally separated and validate allocated widths at representative widths such as `900sp`, `1200sp`, and ultrawide desktop sizes.
- [Risk] Existing tests and docs assume the stored fraction equals the actual visible width. → Mitigation: update tests and docs to distinguish preset identity from effective allocated width.
- [Risk] The properties-pane breakpoint could regress if it still uses the unclamped left fraction. → Mitigation: compute breakpoint math from the effective left width and add widget tests that exercise preset changes and wide-window behavior.
- [Risk] Moving the control into Preferences may reduce immediate discoverability for users who already learned the footer buttons. → Mitigation: use a clearly named Workspace preference row and remove the footer entirely so there is only one authoritative place to change the setting.

## Migration Plan

1. Add the Preferences control and wire it to the existing preset state.
2. Remove the sidebar footer controls after the new preference row is active.
3. Update width calculations so the workspace pane uses the adaptive clamp policy and the properties pane reacts to the effective left width.
4. Keep existing installs working by snapping any stored value to the nearest preset and applying the new policy immediately.
5. Update widget tests and docs to reflect the new preference surface and adaptive width behavior.

Rollback is straightforward: the stored preset values continue to map to the same named presets, so the app can temporarily restore the old footer control and raw-fraction behavior without data loss.

## Open Questions

None currently. The initial clamp values above are intentionally specific enough to implement and verify directly.
