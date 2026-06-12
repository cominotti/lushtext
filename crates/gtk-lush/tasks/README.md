# gtk-lush-tasks

`gtk-lush-tasks` is a `0.0.0` GTK Lush family crate for small gtk-rs task
dispatch primitives.

## Pre-Publication Status

This is the first functional in-tree implementation. It is not a Phase 5b publication-ready
crate and is not a crates.io release candidate. The current
API exists so LushText can prove the extraction boundary before publishing any
GTK Lush crate.

Follow the roadmap in `docs/next/gtk-lush.md`. Adoption-validation evidence
for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

Use this crate for bounded blocking work that must return to the GTK main loop.
Keep domain freshness explicit in the application: tab identity, file path
identity, search generations, persistence ordering, and data-loss policy remain
caller-owned rules.

The worker cap covers both running worker closures and completed results waiting
for their GTK-thread completion callback. When capacity is exhausted, work waits
in a main-thread FIFO until a completion or panic releases a slot.

The crate does not create a runtime, message loop, actor system, or application
state framework.
