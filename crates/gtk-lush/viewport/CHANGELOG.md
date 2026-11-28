# Changelog

## Unreleased

- Guarded non-finite (`NaN`/infinite) `page_size` samples in `ViewportObserver`
  so they cannot poison the stored baseline.
- Removed the extra `Rc` around `Cell<f64>` and switched
  `SignalRegistration::new` to `upcast_ref::<Object>().downgrade()`.
- Added `Debug` for `ViewportObserver`, clarified `len()` and
  `for_scrollable`'s post-parenting precondition, and migrated display-dependent
  observer tests into LushText's headless `gtk_lush_adoption` widget lane.

## 0.0.0

- First functional in-tree pre-publication implementation for scroll-adjustment
  viewport observation and rest-state tracking.
- Adoption-lab and matrix evidence before functional publication.
