## Context

The strip is 8 px wide and each lane is right-anchored, so a lane's left edge is
`strip - width` and a width error equals the left-edge error.

## Decision: round to nearest (ties up)

Candidates on the 8 px strip (fractional width → integer width, |deviation|):

| Lane | Fractional w / x | Nearest w / x | Floor w / x | Ceil w / x |
| --- | --- | --- | --- | --- |
| Bookmark | 8.00 / 0.00 | 8 / 0 (0) | 8 / 0 (0) | 8 / 0 (0) |
| Search | 6.56 / 1.44 | 7 / 1 (0.44) | 6 / 2 (0.56) | 7 / 1 (0.44) |
| Modified | 5.12 / 2.88 | 5 / 3 (0.12) | 5 / 3 (0.12) | 6 / 2 (0.88) |
| Long line | 3.68 / 4.32 | 4 / 4 (0.32) | 3 / 5 (0.68) | 4 / 4 (0.32) |
| Max / total | | 0.44 / 0.88 | 0.68 / 1.36 | 0.88 / 1.64 |

Nearest wins on both the maximum and the total deviation, for widths and x
alike, and keeps four distinct nested lanes (8 > 7 > 5 > 4) inside the strip.
Integer arithmetic: `(total * percent + 50).div_euclid(100)` in `i64`, then the
two-pixel floor, which is unchanged and only binds below a 4 px strip.

## Verification

An exhaustive unit test over strips 4..=4096 px checks nearest rounding (within
half a pixel, in hundredths), nesting, `x + width == strip`, and containment.
No Kani harness covered these functions; none is added.
