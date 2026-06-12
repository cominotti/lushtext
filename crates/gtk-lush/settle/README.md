# gtk-lush-settle

`gtk-lush-settle` provides generation-counter helpers for GTK main-loop work
where the latest request wins.

The crate keeps a small boundary between deterministic state and GLib
scheduling. Generation tokens decide whether a callback is current; GLib's
main loop decides when that callback runs. No custom runtime, executor,
component lifecycle, or message loop is introduced.

```rust
use std::time::Duration;

use glib::Object;
use gtk_lush_settle::Debounce;

let debounce = Debounce::new();
let target = Object::new::<Object>();
let token = debounce.schedule(&target, Duration::from_millis(150), |_, _| {
    // Update a GTK widget after the input burst quiets.
});

assert!(debounce.is_current(token));
```

## Choosing A Primitive

- `Debounce` fits trailing latest-input work such as search queries, filter
  updates, and coalesced persistence scheduling. It also exposes
  advance/invalidate/current-token checks for immediate empty states and async
  freshness gates tied to the same family.
- `SettleBurst` fits readiness-visible repair after noisy layout or rendering
  bursts. Its `pending()` state remains true until the current repair handle
  finishes.
- `SupersedingTimer` fits delayed one-shot cleanup or reveal/hide work where
  each new arm supersedes the previous arm.

## Non-Settle Exceptions

Keep these classes explicit unless a local audit proves they fit the crate and
tests cover the migration:

- recurring pollers and heartbeats;
- chunked yielding or model-population loops;
- long-running async worker freshness tokens not tied to one debounce family;
- domain generations that protect durable writes, undo journals, or persisted
  state ordering;
- animation-frame repair loops where frame ownership matters.

## GTK Lush Limits

GTK Lush crates must pass the afternoon-adoption test: a stock gtk-rs
application can adopt exactly one crate in an afternoon without restructuring
anything else.

This crate has no view DSL, no component system, no state/message loop, no
runtime dependency on another GTK Lush crate, and no replacement for
Libadwaita adaptive behavior.

## Pre-Publication Status

This is the first functional in-tree `0.0.0` API. It is suitable for LushText's
in-repository migration work, but it is not a Phase 5b publication-ready crate
and should not be treated as a stable external dependency yet.

Program roadmap and publication gates live in
[`docs/next/gtk-lush.md`](https://github.com/cominotti/lushtext/blob/main/docs/next/gtk-lush.md)
and
[`crates/gtk-lush/GOVERNANCE.md`](https://github.com/cominotti/lushtext/blob/main/crates/gtk-lush/GOVERNANCE.md).
Adoption-validation evidence for this crate, including the timed stock
fixture, is tracked in
[`docs/gtk-lush-adoption/`](https://github.com/cominotti/lushtext/blob/main/docs/gtk-lush-adoption/).
