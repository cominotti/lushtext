# gtk-lush-proof-spine

`gtk-lush-proof-spine` is a `0.0.0` GTK Lush family crate for bounded
readiness, snapshot, workflow-event, and artifact-envelope protocol types.

## Internal Platform Status

This is a functional in-tree `0.0.0` implementation for LushText's internal
platform. It is not a stable external dependency and is not a crates.io release
candidate. The current API exists so LushText can keep proof-spine contracts
bounded, local, and reviewable.

Follow the current posture in `docs/next/gtk-lush.md`. Baseline adoption
evidence for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

Use this crate for GTK-free proof protocol value objects and provider traits.
Consumer applications own their app state, D-Bus or CLI transport, GTK actions,
snapshot collection, and command execution. This crate does not register a
D-Bus object, wrap widgets, define a view DSL, or introduce an app
state/message framework.
