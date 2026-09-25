## ADDED Requirements

### Requirement: Pure geometry and budget policies do whole-pixel arithmetic, enforced mechanically
A pure geometry or budget policy module SHALL take and return whole pixels
(integers) and SHALL do no floating-point arithmetic, with the GTK adapter
converting to `f64` once at the widget or split-view boundary: a split-view
fraction SHALL be a whole-sp width divided once in the adapter, and a Pango
measure SHALL be passed in Pango units rather than divided into pixels first.
Only a GTK widget coordinate or scroll-adjustment value that is fractional
under scaling or smooth scrolling MAY stay `f64` inside such a module, and the
function computing it SHALL state its domain in its rustdoc.

The protected modules, each with its lint level and its ceiling of admitted
functions, SHALL be the table in the "Whole-pixel geometry policy" section of
`.agents/rules/workflow-convention.md`, which SHALL equal the list the boundary
check enforces; every `policy.rs` under a declared geometry role home
(`ui/window/geometry/`, `ui/editor_page/minimap/`, `ui/markdown_preview/`)
SHALL be protected at ceiling 0 without an edit to either. The rule SHALL be
normative in that convention section, which other rule files point to.

The rule SHALL be enforced in two layers:

- **The compiler.** A protected module with ceiling 0 SHALL carry
  `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`, which
  no attribute beneath it can lower. A module that still admits fractional
  functions SHALL carry the same pair as `deny` and SHALL admit each such
  function only by a reasoned `#[expect(..., reason = "...")]` of one or both
  lints placed directly on that function. The root `clippy.toml` SHALL list, as
  `disallowed-methods` with a reason each, the `f64` and `f32` methods that do
  float arithmetic by call (`mul_add`, `powi`, `powf`, `sqrt`, `div_euclid`,
  `rem_euclid`, `recip`, `hypot`, `midpoint`), and the workspace Clippy table
  SHALL allow `disallowed_methods` so that it applies only where a protected
  module raises it. The all-feature Clippy gate of `make check` then fails on
  any unadmitted float arithmetic, by operator or by method, in a protected
  module.
- **The boundary check.** `make check-workflow-boundaries` SHALL read each
  protected module and each of its child files (`x.rs` → `x/**/*.rs`, except a
  `cfg(kani)`-gated `kani_proofs.rs`) as code with comments and literals
  removed, and SHALL fail when: a ceiling-0 module lacks the `forbid` pair, or
  another protected module lacks the `deny` or `forbid` pair; any attribute
  changes the level of either lint, of the `restriction`, `style`, or `all`
  groups, or of `blanket_clippy_restriction_lints` through `allow`, `warn`,
  `cfg_attr`, an inner `expect`, or an `expect` on an item that is not a
  function (an `impl`, a `mod` or `mod x;` declaration, a `trait`, a
  statement); a module is remapped with `#[path]` or text is pulled in with
  `include!`; the number of functions admitted by an `expect` exceeds the
  module's ceiling; a listed module no longer exists; or the convention's table
  disagrees with the enforced list. Its self-test SHALL cover each of these
  cases.

#### Scenario: A module needing no admission performs float arithmetic
- **WHEN** a function in a ceiling-0 protected module performs floating-point arithmetic, by operator or by a disallowed float method
- **THEN** `make check` fails on the Clippy `float_arithmetic` or `disallowed_methods` error
- **AND** no `allow`, `expect`, or `cfg_attr` anywhere in the module or its children can silence it, because the module forbids both lints

#### Scenario: An admitting module adds an unadmitted fractional function
- **WHEN** a function in a `deny` module performs floating-point arithmetic without its own `expect` of the lint it trips
- **THEN** `make check` fails on the Clippy error

#### Scenario: A new geometry policy module omits the attribute
- **WHEN** a new `policy.rs` is added under `ui/window/geometry/`, `ui/editor_page/minimap/`, or `ui/markdown_preview/` without `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`
- **THEN** `make check-workflow-boundaries` fails, naming the module

#### Scenario: A lowering attribute Clippy's own gates miss is refused
- **WHEN** a protected module or one of its child files expects either lint on an `impl`, an inline `mod`, or a `mod x;` line, expects it module-wide, lowers it through `cfg_attr` or a group, or allows it with an inner attribute
- **THEN** `make check-workflow-boundaries` fails, naming the file and line

#### Scenario: The attribute is only in a comment
- **WHEN** the required `forbid` or `deny` appears only inside a line or block comment
- **THEN** `make check-workflow-boundaries` fails, naming the module

#### Scenario: Admissions exceed the recorded ceiling
- **WHEN** a `deny` module and its child files admit more functions by `expect` than the module's recorded ceiling
- **THEN** `make check-workflow-boundaries` fails, naming the module, the count, and the ceiling

#### Scenario: The convention table drifts from the enforced list
- **WHEN** the convention's whole-pixel table omits, adds, or records a different level or ceiling for a module than the boundary check enforces
- **THEN** `make check-workflow-boundaries` fails, naming the module
