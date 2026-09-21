## 1. Failing tests first

- [x] 1.1 `gtk_lush_adoption.rs`: padded `GtkListView` fixture (CSS class with asymmetric vertical padding); rendered-bounds test resolving a row by label each sample, measured relative to a fixed sibling above the bin, layout forced between samples, selection via `set_selected` and focus via `grab_focus` on already-visible rows, all samples pixel-identical while the outer value is unchanged; at the top and mid-content
- [x] 1.2 `workspace_tree_virtualization.rs`: the same recipe against the real sidebar relative to the section header; static fixtures (short window, two sections, refresh over an identical tree) across the event; model-changing fixtures (expand, collapse, hidden-files) post-settle only
- [x] 1.3 Positive control: a row clipped at the slice bottom is revealed on `scroll_to(FOCUS)` and the outer then rests (the travel is the list's decision; see design findings)
- [x] 1.4 Confirm the new tests fail on v0.8.1; record which and by how much
- [x] 1.5 Commit the failing tests

## 2. Instrumentation (temporary, removed in 4.3 before the fix commit)

- [x] 2.1 Log in `size_allocate`: `viewport_top`, `slice.top` pre-rounding, `slice_top`, `published`, `value` after `child.allocate`, `upper`/`page_size` before and after
- [x] 2.2 Log in `child_adjustment_moved`: `value`, `allocating`, delta, whether the idle applied it
- [x] 2.3 Driven in the widget harness rather than live (the harness reproduces the reported gesture deterministically): settles arrive inside allocation on focus, 1–3px; the out-of-allocation path never fired; the pre-rounding top did not oscillate
- [x] 2.4 Confirm that a deliberate 5px `set_value` moves a row's `compute_bounds` (the rendered-bounds tests depend on it)

## 3. Evidence surface

- [x] 3.1 `allocations_for_test()` and `corrections_for_test()` counters on the bin behind the test-utils feature; document in `README.md`

## 4. C2: publish geometry in the child's frame

- [x] 4.1 Derive the inset after `child.allocate` (`slice_height − page_size` when the child changed `page_size`), store it, invalidate on child replacement and `unroot`; publish `upper = content_height − inset`, `page_size = slice_height − inset`, `value = slice_top` unchanged
- [x] 4.2 Test: after the first allocation, the child's `upper` and `page_size` equal the published ones and `reconfigure_shift` is zero on ordinary allocations
- [x] 4.3 Remove the instrumentation

## 5. Residual

- [x] 5.1 Unconditional re-assert of `published` after the request decision is read, with `allocating` moved to after the last value the bin sets; remove the `published_offset := value` re-baseline
- [x] 5.2 Allocation-count-at-rest test: no growth over N forced flushes once settled; at most one extra per correction
- [x] 5.3 Not needed: the out-of-allocation path did not fire (recorded in `design.md`)
- [x] 5.4 Not needed: no rounding oscillation observed

## 6. Regression

- [x] 6.1 All new tests pass; wheel round-trips, focus traversal, header tests, positive control stay green
- [x] 6.2 Proved: the joint revert fails (commit `d046c44b`); each half alone passes — recorded as a finding in `design.md`, both halves kept
- [x] 6.3 Live confirmation is the user's Flatpak update at release (v0.8.2); the harness reproduces both the "stays" and "comes back" cases deterministically and both are green

## 7. Documentation and gates

- [x] 7.1 `AGENTS.md` viewport-slice decision (content-box inset, the invariant, the retracted hypotheses), the `gtk-lush-viewport-slice` spec, `scroll_request.rs` doc block
- [x] 7.2 GTK Lush governance: `crates/gtk-lush/widgets/CHANGELOG.md` and `README.md`; review the `make gtk-lush-public-api-advisory` diff; `make check-gtk-lush-policy`, `check-gtk-lush-adoption`, `gtk-lush-doctests`, `gtk-lush-examples`
- [x] 7.3 Visual-geometry lane once per distinct visual-sensitive file set before each commit; `make check` and `make test-widget` green
