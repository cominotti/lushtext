## Context

LushText's persistent bottom status bar currently spans the full window width below the shrinkable editor/sidebar shell. It owns three compact lanes: the workspace-sidebar toggle, the status-message area, and document metadata controls such as `EditorConfig`, line endings, and encoding.

The current implementation intentionally keeps the bar compact: the CSS sets a small minimum height, the message and metadata use caption styling, and the sidebar toggle is a subdued flat button. That keeps editing space high, but it also makes the status strip feel tighter than the header bar and can reduce readability for status messages and metadata controls.

The product direction is to make the status bar easier to read while preserving its role as secondary chrome. It should feel calmer and more legible, not like a second primary header bar.

## Goals / Non-Goals

**Goals:**

- Increase the status bar's vertical comfort enough that messages and compact metadata are easier to read.
- Preserve a one-row bottom status strip with lower prominence than the header bar.
- Preserve the existing status-bar structure: workspace toggle, gap, message lane, metadata controls.
- Preserve current notification flash behavior, severity contrast, and message-area boundaries.
- Cover visual extremes: no active document, normal document metadata, long messages, narrow widths, short heights, light/dark styling, and high contrast.

**Non-Goals:**

- Do not make the status bar match the header bar's height or visual priority by default.
- Do not introduce a second toolbar, extra row, title, view switcher, or new commands in the status bar.
- Do not move slower inspection details such as file size, formatting source, statistics, or file-health review out of document properties.
- Do not change notification ownership, message expiry semantics, automation payloads, persistence, or application actions.

## Decisions

### Modestly resize the existing one-row status bar

Keep the current `LushtextStatusBar` widget and tune its Blueprint margins/CSS sizing rather than replacing it with a different shell pattern. A small increase in minimum height and internal vertical spacing is enough to improve readability while keeping the bar subordinate.

Alternative considered: match the header bar height. Rejected because it gives persistent status and metadata chrome the same visual weight as primary window controls, and it makes empty/no-document states look overbuilt.

### Keep caption-adjacent density, but improve centering and touch/scan comfort

The implementation should avoid large type or header-style button treatment. Prefer slightly roomier vertical margins, stable button dimensions, and centered alignment so labels feel less cramped without becoming toolbar controls.

Alternative considered: switch message and metadata labels to normal body-size text. Rejected unless visual proof shows caption sizing remains unreadable after spacing changes, because body text would increase prominence and crowd narrow windows.

### Treat state extremes as acceptance criteria

The status bar must remain visually coherent when metadata is hidden, when metadata is populated, when messages are long or severity-flashing, and when the window is narrow or short. The existing short-window contract is especially important because the status bar is intentionally preserved while the central shell clips.

Alternative considered: validate only the populated happy path. Rejected because the no-document and constrained-height states are where a taller persistent strip is most likely to look awkward or consume too much editor space.

### Preserve existing notification boundaries

The message-area flash must continue to cover the message lane only, excluding the workspace toggle, the left gap, and metadata controls. Layout tuning must not merge those areas visually or change the notification bus semantics.

Alternative considered: flash the full new taller bar for stronger acknowledgement. Rejected because the current contract deliberately avoids flashing controls and keeps severity feedback scoped to the message lane.

## Risks / Trade-offs

- Taller chrome reduces editor space in short windows -> keep the height increase modest and verify short/tiny window tests still preserve the status bar without making the editor unusable.
- More vertical padding may make the status bar feel like a toolbar -> keep caption-adjacent typography, subdued metadata styling, and a one-row layout.
- Long localized metadata or messages may crowd the bar at narrow widths -> preserve message ellipsizing, keep metadata terse, and verify no horizontal scrollbar or wrapped second row appears.
- High contrast may expose weak spacing or color assumptions -> include high-contrast visual or widget coverage before implementation is considered done.
