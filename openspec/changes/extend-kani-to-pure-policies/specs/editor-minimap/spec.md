## ADDED Requirements

### Requirement: Minimap fit functions are machine-checked on a stated domain
The project SHALL keep Kani harnesses over the production minimap fit
functions: `fit_native_slider_to_source_map_bounds`, `fit_marker_bounds`,
`fit_projected_bounds`, `expanded_to_min_height`, and
`native_slider_estimate_from_inputs`. The harnesses SHALL prove that none of
these functions panics for any `f64` input, including non-finite values, or any
`i32` input. On a whole-pixel domain the harnesses SHALL prove containment and
the minimum-height guarantee. In that domain every coordinate is an integer
with magnitude at most 2^20, and every minimum height is an integer from 0 to
2^16. They SHALL prove:

- a returned marker or projected rectangle lies inside the rendered-content
  band, `lower <= top < bottom <= upper`. Here `lower` is
  `max(content_top, 0)`, and `upper` is `min(content_bottom, h)`, where `h` is
  the strip height or the target height;
- a returned rectangle's height is positive and at least the smaller of the
  minimum height and the band height;
- `RejectOutside` returns nothing for a span entirely outside the band;
- the fitted native slider keeps its horizontal position and width and lies
  vertically inside the source-map bounds.

When one of these properties fails for general finite `f64` inputs, the
counterexample SHALL be kept as a `should_panic` harness. The function's
rustdoc SHALL then state the whole-pixel domain on which the property is
proved.

#### Scenario: Markers never reach the EOF overscroll tail
- **WHEN** the harness explores every whole-pixel marker span and projection space within its domain
- **THEN** no returned marker's bottom lies below the last rendered content line (`bottom <= min(content_bottom, strip_height)`)

#### Scenario: Non-finite geometry never panics
- **WHEN** any fit function receives NaN or infinite coordinates
- **THEN** it returns without panicking, and it returns no bounds unless every coordinate it returns is finite
