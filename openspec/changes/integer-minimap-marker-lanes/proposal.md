## Why

`marker_lane_width` and `marker_lane_x` were the last two functions in the
minimap's whole-pixel policy that did float arithmetic, each admitted by a
function-level `#[expect(clippy::float_arithmetic, ...)]`. `extend-kani-to-pure-policies`
left making them whole-pixel as a maintainer option because it changes what is
drawn. The maintainer took the option, choosing the integer lanes closest to
today's look.

## What Changes

- The four marker lanes (bookmark 100%, search 82%, modified 64%, long line
  46% of the strip) are rounded to the nearest whole pixel, ties up, with the
  existing two-pixel floor. On the shipped 8 px strip the lanes go from
  8 / 6.56 / 5.12 / 3.68 px to 8 / 7 / 5 / 4 px, right-anchored at
  x = 0 / 1 / 3 / 4.
- Both functions take and return `i32`; the drawing code in
  `projection_execution.rs` converts to `f64` once at the cairo call.
- The minimap's admission ceiling drops from 10 to 8 in
  `scripts/check-workflow-boundaries.py` and the convention's whole-pixel table.

## Impact

- `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs`,
  `projection_execution.rs`; `scripts/check-workflow-boundaries.py`;
  `.agents/rules/workflow-convention.md`; `docs/next/formal-verification.md`.
- Visible: lane edges become crisp on whole pixels; the search lane widens by
  0.44 px, modified narrows by 0.12 px, long line widens by 0.32 px. Colours,
  order, and vertical projection are unchanged.
