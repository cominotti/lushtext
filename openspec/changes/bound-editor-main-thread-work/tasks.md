## 1. Live Periodic Local-History Capture

- [ ] 1.1 Add pure live-buffer history classification for full periodic, save-boundary-only, and unavailable states using current conservative buffer size.
- [ ] 1.2 Add editor/path/periodic/edit generations to periodic capture and choose the shared direct or chunked snapshot path from live state.
- [ ] 1.3 Reject capture when the editor closes, identity changes, eligibility changes, periodic generation changes, or edits occur during chunking before worker persistence begins.
- [ ] 1.4 Add threshold, stale edit, Save As/rename, close, Unicode, and rescheduling tests while preserving deduplication and save-boundary capture.

## 2. Bounded Find and Replace Prefill

- [ ] 2.1 Add a named 1,024-character in-editor selection-prefill limit and check selection offsets before copying text.
- [ ] 2.2 Prefill accepted non-empty selections exactly and leave the existing query focused/selected but unchanged for oversized selections.
- [ ] 2.3 Add Find and Replace widget tests for empty, exact-limit, one-over, large, multibyte Unicode, existing-query, and repeated-open states.

## 3. Byte Scan and Markdown Fast Paths

- [ ] 3.1 Replace scalar CR/LF candidate scanning in line-ending detection with `memchr2_iter`, including correct CRLF skip behavior.
- [ ] 3.2 Add equivalence/property tests and benchmark coverage for empty, LF, CRLF, CR, mixed, boundary, and large decoded text.
- [ ] 3.3 Add Markdown rendered-embed generation and last processed valid `(column width, embed generation)` state.
- [ ] 3.4 Invalidate embed generation on render/clear/placeholder/membership changes and skip traversal only when both cache inputs are unchanged.
- [ ] 3.5 Preserve immediate/idle/timed geometry passes and final readiness callbacks, including hidden zero-width and late valid allocation behavior.

## 4. Responsiveness and Geometry Proof

- [ ] 4.1 Add a many-code-block performance fixture proving unchanged deferred passes avoid repeated embed traversal and new embeds still receive widths.
- [ ] 4.2 Add visual/widget geometry tests for root and nested code blocks across hidden, preview-only, side-by-side, resized, and constrained states.
- [ ] 4.3 Run formatting, responsiveness/performance/architecture reviews, focused tests and benchmarks, `make visual-geometry-smoke`, `make check`, and `make pre-commit`; fix every issue found.
- [ ] 4.4 Run the learning workflow and update local-history or preview guidance only if implementation establishes a new durable rule.
