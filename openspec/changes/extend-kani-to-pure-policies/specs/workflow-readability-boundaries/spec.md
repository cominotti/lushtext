## ADDED Requirements

### Requirement: Pure geometry and budget policies do whole-pixel arithmetic, enforced mechanically
A pure geometry or budget policy module SHALL take and return whole pixels
(integers) and SHALL do no floating-point arithmetic, with the GTK adapter
converting to `f64` once at the widget or split-view boundary. A value that is
genuinely fractional (a preset hint fraction of a window width, a split-view
fraction, or a GTK widget coordinate or scroll-adjustment value that is
fractional under scaling) MAY stay `f64`, and the function computing it SHALL
state its domain in its rustdoc.

The rule SHALL be enforced in two layers:

- every such module SHALL carry `#![deny(clippy::float_arithmetic)]`, so the
  all-feature Clippy gate of `make check` fails on unadmitted float
  arithmetic; a fractional value SHALL be admitted only by a
  `#[expect(clippy::float_arithmetic, reason = "...")]` on the one function
  that computes it, and never by an `allow` of the lint or a module-wide
  `expect`; an `allow` of the lint, and an `expect` without a reason, are
  already refused by the workspace Clippy lints `allow_attributes` and
  `allow_attributes_without_reason`;
- `make check-workflow-boundaries` SHALL fail when a module in its declared
  whole-pixel list, or any `policy.rs` under a declared geometry role home
  (`ui/window/geometry/`, `ui/editor_page/minimap/`, `ui/markdown_preview/`),
  lacks that attribute or expects the lint module-wide (which both workspace
  lints accept), and when a listed module no longer exists. Its self-test
  SHALL cover each of these cases.

The declared whole-pixel list SHALL contain at least
`ui/window/geometry/policy.rs`, `ui/editor_page/minimap/policy.rs`,
`ui/markdown_preview/policy.rs`, `ui/sidebar/width_preset.rs`,
`model/editor_memory.rs`, and, because convention amendments apply
retroactively, the other migrated policies that hold geometry decisions:
`ui/window/local_history/policy.rs` and `ui/window/focus_mode/policy.rs`. The rule SHALL be normative in
`.agents/rules/workflow-convention.md`, which other rule files point to.

#### Scenario: A geometry policy performs float arithmetic
- **WHEN** a function in a listed module performs floating-point arithmetic without a function-level `expect` of `clippy::float_arithmetic`
- **THEN** `make check` fails on the Clippy `float_arithmetic` error

#### Scenario: A new geometry policy module omits the deny
- **WHEN** a new `policy.rs` is added under `ui/window/geometry/`, `ui/editor_page/minimap/`, or `ui/markdown_preview/` without `#![deny(clippy::float_arithmetic)]`
- **THEN** `make check-workflow-boundaries` fails, naming the module

#### Scenario: A module-wide escape hatch is refused
- **WHEN** a listed module expects `clippy::float_arithmetic` module-wide
- **THEN** `make check-workflow-boundaries` fails, naming the module

#### Scenario: An allow, or a reasonless expect, is refused
- **WHEN** a listed module allows `clippy::float_arithmetic`, or expects it without a reason
- **THEN** `make check` fails on the Clippy `allow_attributes` or `allow_attributes_without_reason` error
