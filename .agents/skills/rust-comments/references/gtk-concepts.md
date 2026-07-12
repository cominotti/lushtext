# GTK and GLib Commenting Concepts

Use this reference only when a changed mechanism has a consequence that names
and types do not reveal. Do not paste these explanations at every first use.

## Table of Contents

- [GObject state and ownership](#gobject-state-and-ownership)
- [Signals](#signals)
- [Main-thread scheduling](#main-thread-scheduling)
- [Templates and type registration](#templates-and-type-registration)
- [Models and factories](#models-and-factories)
- [Settings](#settings)

## GObject state and ownership

`glib::wrapper!` exposes the public Rust wrapper around the implementation
object. Comment it only when an unusual inheritance or interface choice affects
callers.

GObject methods commonly receive `&self`, so implementation state uses `Cell`
or `RefCell`. Explain the lifecycle or invariant of a non-obvious field, not the
basic definition of interior mutability.

Weak references break ownership cycles between widgets and callbacks. Comment
what must still happen when an upgrade fails if silently skipping the callback
would alter lifecycle behavior.

## Signals

Signal callbacks run according to the emitting object's context. Explain:

- why a handler must be disconnected or blocked;
- why reentrancy is guarded;
- which object owns the handler lifetime;
- why `connect_notify_local` is required for a non-`Send` capture.

Do not label every `connect_*` call as an observer pattern.

## Main-thread scheduling

GTK objects belong to the main thread. For `idle_add_once`, timers, or a
background-to-main callback, document the state snapshot and freshness check
when delayed delivery could otherwise apply stale work.

Do not assert a particular worker queue, priority, or timeout implementation in
a comment unless the code itself owns that contract.

## Templates and type registration

`CompositeTemplate` binds resource-defined children to a widget subclass.
Explain `ensure_type()` when registration order is load-bearing: template
parsing must know a custom type before constructing it.

Do not narrate standard `ObjectSubclass`, `ObjectImpl`, `TemplateChild`, or
`class_init` boilerplate.

## Models and factories

GTK list factories recycle row widgets. Comment any reset or rebinding logic
whose absence would leak state from one model item into another.

`GtkTreeListModel`, `GtkListView`, and `GtkTreeExpander` divide hierarchy,
virtualized presentation, and disclosure UI. Explain only cross-layer behavior
such as why a store update must preserve row identity or expansion state.

Use a comment for `ListStore::splice()` when batching is required to preserve a
specific signal, selection, or performance invariant; do not claim it is always
preferable.

## Settings

`gio::Settings::bind()` projects a setting through GObject properties. Explain
mapped bindings, override precedence, or side effects that prevent a direct
two-way binding. Straight property-to-setting bindings are self-explanatory.
