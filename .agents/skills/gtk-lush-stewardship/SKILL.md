---
name: gtk-lush-stewardship
description: "Steward GTK Lush as LushText's internal platform. Use when adding, changing, or reviewing GTK Lush crates or APIs; deciding whether repeated GTK helper code belongs in GTK Lush; refreshing GTK Lush adoption evidence, README, CHANGELOG, policy, public-API advisory, proof, or governance docs; reopening publication, repository graduation, or upstreaming; or working in crates/gtk-lush/**, crates/gtk-lush-adoption-lab/**, fixtures/gtk-lush-adoption/**, docs/gtk-lush-adoption/**, crates/cargo-gtk-proof/**, or docs/next/gtk-lush.md. Do not use for ordinary LushText feature work unless it proposes a GTK Lush boundary decision."
---

# GTK Lush Stewardship

GTK Lush is currently stable in-tree LushText infrastructure. It is useful even
if it never publishes functional crates. Stewardship means preserving that
value without turning every LushText feature into an extraction project.

## Current Posture

- Functional GTK Lush crates stay `0.0.0` workspace APIs consumed through path
  dependencies.
- Publication, `0.1.0`, repository graduation, published LushText
  dependencies, and broad upstreaming are dormant tracks. They require a
  dedicated maintainer-approved OpenSpec change with refreshed evidence.
- Existing local gates remain first-class: policy, adoption, doctests,
  examples, MSRV, public-API advisory, proof, and `make check`.

## Use Existing Primitives First

For ordinary LushText feature work, check whether an existing GTK Lush crate
already owns the pattern:

- signal, binding, and controller lifetimes -> `gtk-lush-signals`
- UI debounce, superseding timers, and readiness-settle -> `gtk-lush-settle`
- bounded background work returning to GTK -> `gtk-lush-tasks`
- adjustment observation, rest state, and lower-edge geometry ->
  `gtk-lush-viewport`
- zero-min clipping and render-hold/capture overlays -> `gtk-lush-widgets`
- widget harness and proof value objects -> `gtk-lush-proof-harness` and
  `gtk-lush-proof-spine`
- visual proof schemas, corpus replay, policy, and same-session proof ->
  `cargo-gtk-proof`

If a primitive fits, use it. If it almost fits, prefer app-local code unless
there is a real stewardship signal.

## Approve New Or Changed GTK Lush API Only With A Signal

Before proposing or implementing a new GTK Lush API, name at least one:

- current LushText pain that repeats across call sites
- adoption evidence or policy drift
- proof-tooling improvement that materially increases UI confidence
- real external adopter need

Do not add GTK Lush API only because an older roadmap had a next phase, a
helper repeats twice, or publication would be tidier.

## Constitution Checklist

Every GTK Lush crate/API change must preserve:

- no ownership of GTK control flow
- no custom view DSL
- no state, message, or component framework
- no runtime dependency on another GTK Lush family crate
- no replacement for Libadwaita adaptive behavior
- proof appropriate to the surface

Record exceptions in `crates/gtk-lush/GOVERNANCE.md` before merge.

## Evidence And Docs To Keep In Sync

When a GTK Lush API, example, fixture, lab workflow, proof schema, or policy
check changes, update the matching evidence in the same change:

- crate README and CHANGELOG
- examples and doctests
- `docs/gtk-lush-adoption/matrix.toml`
- adoption lab or stock fixture if affected
- `docs/gtk-lush-adoption/api-review.md` for accepted limitations
- public-API advisory snapshots when API shape changes
- `crates/gtk-lush/GOVERNANCE.md` for review posture or exceptions
- `docs/next/gtk-lush.md` and OpenSpec specs for posture or scope changes

Keep generated artifacts, external checkouts, screenshots, and large logs out
of git unless they are curated bounded fixtures.

## Verification

Choose the narrowest lane that matches the change, then broaden when risk
requires it:

```sh
make check-gtk-lush-policy
make check-gtk-lush-adoption
make gtk-lush-doctests
make gtk-lush-examples
make gtk-lush-msrv
make gtk-lush-api-advisory
make check-agent-docs
make check
git diff --check
```

For rendered geometry or proof-tooling behavior, add the relevant widget or
visual proof lane before signing off.
