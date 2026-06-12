# gtk-lush-proof-spine

`gtk-lush-proof-spine` is a `0.0.0` GTK Lush family crate for bounded
readiness, snapshot, workflow-event, and artifact-envelope protocol types.

## Pre-Publication Status

This is the first functional in-tree implementation. It is not a Phase 5 publication-ready
crate and is not a crates.io release candidate. The current API exists so
LushText can prove the extraction boundary before publishing any GTK Lush
crate.

Follow the roadmap in `docs/next/gtk-lush.md`.

## Scope

Use this crate for GTK-free proof protocol value objects and provider traits.
Consumer applications own their app state, D-Bus or CLI transport, GTK actions,
snapshot collection, and command execution. This crate does not register a
D-Bus object, wrap widgets, define a view DSL, or introduce an app
state/message framework.
