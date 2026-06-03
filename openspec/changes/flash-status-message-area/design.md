## Context

`LushtextStatusBar` currently renders status feedback by applying severity CSS classes directly to `message_label`. The label expands across the middle of the bottom bar, but only the text itself visibly changes when the same notification is published again. The window notification bridge already distinguishes publish/update paths from sweep/resolve paths, and the search-progress heartbeat can re-render unchanged visible text.

The desired behavior is an acknowledgement pulse for the visible status-bar notification. The pulse must cover the whole message lane between the workspace toggle and document metadata controls, including empty horizontal space, without making unrelated status-bar controls look active.

## Goals / Non-Goals

**Goals:**

- Flash the full horizontal message area whenever a visible status-bar notification is newly published or meaningfully updated.
- Use severity-specific info, warning, and error colors with readable foreground contrast during the pulse.
- Restart the pulse for rapid repeated identical messages such as repeated Save actions.
- Keep unchanged progress heartbeats, notification expiry, and hidden-under-transient progress updates from causing distracting flashes.
- Preserve the existing status-bar layout, metadata controls, and caption-sized message text.

**Non-Goals:**

- Changing notification text, adding counters, sounds, icons, or persistent badges.
- Replacing the editor inline-alert surface or changing inline alert behavior.
- Introducing a new notification storage model or external dependency.
- Highlighting the workspace toggle or document metadata controls.

## Decisions

### Wrap the label in a full-width message area

Add a template child around `message_label`, for example `message_area_box`, with `hexpand=true`. Move the label's visual padding inside that wrapper so the wrapper's background spans the entire available message lane while the text keeps its current offset and ellipsizing behavior.

Alternative considered: animate `message_label` directly. That would leave most of the empty message lane visually unchanged and would not satisfy the requirement that the whole available message area highlight.

Alternative considered: animate the root `.status-bar`. That would also highlight the workspace toggle and metadata controls, making unrelated controls feel active.

### Keep rendering and pulsing separate

Keep `render_message` responsible for the steady visible message and add a separate pulse method on `LushtextStatusBar`. The window notification bridge should call the pulse method only after publish/update paths whose payload is the visible status-bar view. Sweeps, resolves, and search heartbeat renewals should continue to render without pulsing.

Alternative considered: pulse from every `render_notifications()` call. That would make search-progress heartbeats and expiry maintenance visually noisy because those paths can re-render without a user-visible new notification.

### Restart repeated pulses with generation plus alternating classes

Store a small generation counter and an alternating pulse flag in the status-bar widget. Each pulse removes any previous pulse classes, increments the generation, toggles between two equivalent pulse classes, and schedules cleanup only if the captured generation is still current. Alternating classes gives GTK a fresh CSS animation identity even when the same severity and same text repeat quickly.

Alternative considered: append a counter to the text. That would expose implementation detail to the user and clutter short feedback messages.

Alternative considered: remove and re-add one CSS class immediately. GTK may coalesce same-frame style changes, which can fail to restart a running animation reliably.

### Express the flash through app CSS

Use app CSS classes for the pulse background and foreground contrast. Info should use the Adwaita accent palette, warning should use warning palette tokens, and error should use error palette tokens. During the pulse, scoped selectors can temporarily set the child label foreground to a contrasting color; after the animation, the existing steady severity text colors remain in effect.

Alternative considered: drive color changes manually from Rust. CSS keeps the behavior declarative, theme-aware, and local to the status-bar styling surface.

## Risks / Trade-offs

- Animation restart may be flaky if class changes are coalesced → Use alternating pulse classes and generation-guarded cleanup.
- Strong warning/error backgrounds could reduce text contrast in some themes → Use paired foreground tokens during the pulse and verify in light and dark style modes.
- Search progress could become visually noisy → Pulse only from explicit visible publish/update paths, not heartbeat or generic render paths.
- Message-area wrapper could alter status-bar geometry → Keep the wrapper hexpand behavior equivalent to the current label and keep metadata controls outside the pulse wrapper.
- Widget tests may not wait for real animation frames reliably → Assert CSS class application/removal state through deterministic generation or test-time timing rather than depending on rendered animation progress.
