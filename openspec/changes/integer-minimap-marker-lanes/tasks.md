## 1. Whole-pixel lanes

- [x] 1.1 Make `marker_lane_width` / `marker_lane_x` `i32`, round to nearest, and drop their two `expect`s
- [x] 1.2 Convert to `f64` once at the cairo call in `projection_execution.rs`
- [x] 1.3 Replace the lane unit test with integer expectations and an exhaustive nearest/nesting/containment test
- [x] 1.4 Lower the minimap ceiling from 10 to 8 in `scripts/check-workflow-boundaries.py` and `.agents/rules/workflow-convention.md`
- [x] 1.5 Record the taken option in `docs/next/formal-verification.md`

## 2. Verification

- [x] 2.1 `make check`, `make test`, `make kani KANI_SHARD=core-geometry-policies`
- [x] 2.2 `make visual-geometry-smoke` headless for the native-minimap invariants
