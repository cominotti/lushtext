# gtk-lush-settle

`gtk-lush-settle` is a `0.0.0` placeholder reservation for the future GTK Lush
settle/debounce crate.

GTK Lush turns LushText's GTK4/Libadwaita rules into small, independently
adoptable Rust crates. Every crate must pass the afternoon-adoption test: a
stock gtk-rs application can adopt exactly one crate in an afternoon without
restructuring anything else.

This package intentionally exposes no public API yet. The planned follow-up
OpenSpec change is `extract-gtk-lush-signals-and-settle`; it will design the
debounce, settle-burst, and superseding-timer helpers against
the [GTK Lush governance document](https://github.com/cominotti/lushtext/blob/main/crates/gtk-lush/GOVERNANCE.md)
and the umbrella vision in
[`docs/next/gtk-lush.md`](https://github.com/cominotti/lushtext/blob/main/docs/next/gtk-lush.md).

## Constitution

- No GTK control-flow ownership.
- No custom view DSL.
- No state/message/component system.
- No runtime dependency on another GTK Lush crate.
- No reimplementation of Libadwaita adaptive behavior.
- Proof over claims: docs, tests, and visual evidence where rendering is
  involved.

## Placeholder Status

Do not use this `0.0.0` crate for application logic. It exists only to reserve
the package name and prove workspace scaffolding.
