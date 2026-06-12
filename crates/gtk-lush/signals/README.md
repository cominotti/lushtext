# gtk-lush-signals

`gtk-lush-signals` provides small RAII owners for gtk-rs signal handlers,
`glib::Binding` values, and registration-like cleanup callbacks.

GTK signal connections and property bindings are lifecycle objects even when
they are represented by plain ids or values. This crate gives applications a
place to group those registrations and clear them when a widget, list row,
dialog, or workflow is rebound or torn down.

```rust
use std::cell::Cell;
use std::rc::Rc;

use gio::prelude::*;
use gtk_lush_signals::SignalBag;

let action = gio::SimpleAction::new("count", None);
let hits = Rc::new(Cell::new(0));
let bag = SignalBag::new();

bag.track(&action, action.connect_activate({
    let hits = Rc::clone(&hits);
    move |_, _| hits.set(hits.get() + 1)
}));

action.activate(None);
bag.clear();
action.activate(None);

assert_eq!(hits.get(), 1);
```

## Choosing A Bag

- `SignalBag` owns `SignalHandlerId` values returned by gtk-rs `connect_*`
  calls. Sources are held weakly so the bag can safely outlive a widget or
  object that has already been finalized.
- `BindingBag` owns `glib::Binding` values and unbinds them on `clear()` or
  drop. It fits recycled list rows and widgets that rebind projections.
- `RegistrationBag` owns explicit one-shot cleanup callbacks for controller,
  row-local, or transient registrations that are not represented by a signal id
  or binding.

Each bag is intentionally simple. It does not wrap the gtk-rs signal API,
invent broad `connect_*` traits, own GTK control flow, or hide where callbacks
are installed. Use ordinary gtk-rs APIs first, then record the returned
registration in the right bag.

## Retained Explicit Classes

Not every lifetime should move here. Keep ownership explicit for registrations
where the application needs domain-specific ordering, source removal, worker
thread cancellation, or protocol state beyond disconnect/unbind/cleanup.
Examples include recurring pollers, async freshness tokens, file-save
generations, and non-GObject resources.

## GTK Lush Limits

GTK Lush crates must pass the afternoon-adoption test: a stock gtk-rs
application can adopt exactly one crate in an afternoon without restructuring
anything else.

This crate has no view DSL, no component system, no state/message loop, no
runtime dependency on another GTK Lush crate, and no replacement for
Libadwaita adaptive behavior.

## Internal Platform Status

This is the first functional in-tree `0.0.0` API. It is suitable for the
current LushText internal platform, but it is not a stable external dependency.

Current posture and dormant publication gates live in
[`docs/next/gtk-lush.md`](https://github.com/cominotti/lushtext/blob/main/docs/next/gtk-lush.md)
and
[`crates/gtk-lush/GOVERNANCE.md`](https://github.com/cominotti/lushtext/blob/main/crates/gtk-lush/GOVERNANCE.md).
Baseline adoption evidence for this crate is tracked in
[`docs/gtk-lush-adoption/`](https://github.com/cominotti/lushtext/blob/main/docs/gtk-lush-adoption/).
