# Design: adaptive-inline-alert-layout

## Context

`LushtextInfoBar` is the editor-page driving adapter that turns `InlineActionNotification` values into an inline alert above the editor. Its template (`resources/ui/info-bar.ui`) is a `GtkRevealer` → vertical `alert_box` → `header_box` (message) then `actions_box` (retry/discard/save/dismiss). The vertical `alert_box` is what forces the message and actions onto separate rows at every width.

The alert lives above the center editor column, whose width is highly variable: the workspace sidebar, document-properties pane, and Markdown preview can each consume horizontal space (see `ui.md` split-view rules). A prior attempt at a single horizontal row of message + all controls was rejected in `refine-inline-alert-actions` because a tightly constrained row can starve buttons. This change keeps that lesson but lets the toolkit, not a fixed stack, decide when there is room for one line.

## Goals / Non-Goals

**Goals:**
- Show the message and the action group on one line when the editor column is wide enough.
- Wrap the action group beneath the message when the column is too narrow.
- Guarantee the action group wraps as one atomic unit; its buttons never split across rows and each visible control keeps a positive allocation.
- Reuse the toolkit primitive (`AdwWrapBox`) rather than hand-rolled breakpoint math.
- Preserve notification payloads, callbacks, accessibility behavior, styling, and GTK5-safe widgets.

**Non-Goals:**
- Do not change the notification bus, `InlineActionNotification`, or editor/window callback routing.
- Do not change alert wording or workflow semantics.
- Do not switch the surface to `AdwBanner` (single button, no body) or `AdwAlertDialog`.
- Do not touch status-bar, search-panel, dialog, or toast notifications.
- Do not add a dependency or change feature flags.

## Decisions

### Wrap message and actions in `AdwWrapBox`

Replace the vertical `alert_box` stack with an `AdwWrapBox` whose two children are `message_box` and `actions_box`. `AdwWrapBox` lays children out like words in a wrapping label: both stay on one line when they fit, and the second child wraps to a new line when they do not. This yields the wide single-line look and the narrow stacked look from one container, without manually swapping orientation on a breakpoint.

`AdwWrapBox` is a libadwaita widget added in 1.7. It is not on GTK's GTK5 removal list (that list covers GTK's own widgets), it is not deprecated, and it is available under the workspace `libadwaita` `v1_9` feature (cumulative gates already enable `v1_7`). The custom widget must register `AdwWrapBox` so the template can instantiate it (ensure the type at class init).

### Keep the action group atomic

`actions_box` remains a single horizontal `GtkBox` and is added to `AdwWrapBox` as one child. Because `AdwWrapBox` wraps whole children, the action cluster moves as a unit — it can occupy its own row beneath the message, but its internal buttons never land on different rows. This satisfies the existing spec rule that visible controls must not split into separate rows and each must receive a positive allocation. The discard/save `GtkSizeGroup` and the dismiss-last ordering are unchanged.

### Drive wrap from real width; validate label sizing

`AdwWrapBox` decides to wrap from child natural sizes. The message child holds wrapping `GtkLabel`s (title/body), so its size negotiation needs checking: the goal is that the action cluster stays beside the message until the column genuinely cannot fit both, then wraps — not that it wraps prematurely or that the message compresses to a sliver. Tuning levers if needed: `AdwWrapBox` `child-spacing` / `line-spacing` / `natural-line-length`, the message child's `hexpand`, and label width constraints. This is the main behavior to verify live.

### Rejected alternative: breakpoint-driven orientation swap

An `AdwBreakpoint` that flips the outer box orientation at a width threshold was considered. It was rejected because it duplicates the allocation-frame-churn hazards the project already documents (`widget-wiring.md`: reparsing/reinstalling breakpoint conditions per frame) and re-introduces a hand-picked threshold, whereas `AdwWrapBox` derives the wrap point from measured natural sizes. The breakpoint approach remains the fallback if `AdwWrapBox` cannot be tuned to a clean wrap point.

### Keep the public widget API and rendering stable

`render_notification`, `connect_retry`, `connect_save`, `connect_discard`, and `connect_dismissed` are unchanged. Button visibility logic is unchanged: dismiss is always visible, workflow buttons depend on the payload. Only the container that hosts message + actions changes.

## Risks / Trade-offs

- [Risk] `AdwWrapBox` natural-size wrapping may trigger too early or too late given wrapping labels in the message child. -> Mitigation: validate at representative editor widths; adjust message-child sizing / `AdwWrapBox` properties; this is polish, so fall back to the current always-stacked layout if a clean threshold cannot be found.
- [Risk] Revealer-hosted layout changes can emit `Trying to measure GtkBox ...` / pixman warnings. -> Mitigation: run the live `make run` stderr check on this banner per project rules, not only headless widget tests.
- [Risk] A single-line layout could still crowd buttons at medium widths. -> Mitigation: keep action-label wrapping and assert positive per-button allocation in narrow-layout tests.
- [Risk] Missing `AdwWrapBox` type registration -> template load failure. -> Mitigation: ensure the type at class init and assert the container type in a widget test.

## Migration Plan

1. Register `AdwWrapBox` and restructure `resources/ui/info-bar.ui` so an `AdwWrapBox` hosts `message_box` and `actions_box`.
2. Adjust any spacing in `resources/style/style.css` for the wrap container; keep `.editor-inline-alert` and `.inline-alert-button` rules.
3. Verify the template with `gtk4-builder-tool validate`.
4. Add/extend widget tests: wide single-row, narrow wrap, atomic action group with positive allocations, `AdwWrapBox` container present, no `GtkInfoBar`.
5. Update `.agents/rules/ui.md` (Inline Alerts + widget hierarchy) and `AGENTS.md` if the hierarchy is documented there.
6. Run focused inline-alert tests, `make check`, headless widget tests, and a live `make run` warning capture.

Rollback is local to the template, CSS, type registration, and tests: restore the vertical `alert_box` stack while keeping the rest.

## Open Questions

- ~~What is the cleanest wrap point given the message child's wrapping labels — does the message child need explicit sizing, or do `AdwWrapBox` defaults already wrap at a sensible width?~~ **Resolved during implementation:** `AdwWrapBox` defaults (`wrap-policy=natural`) wrap exactly when the message's one-line natural width plus the action group no longer fit, with no explicit message sizing required. `justify=spread` + `justify-last-line` produce the trailing-action look on the shared line. The breakpoint-orientation-swap fallback was not needed. Validated by wide/narrow widget tests and a live `make run` check.
