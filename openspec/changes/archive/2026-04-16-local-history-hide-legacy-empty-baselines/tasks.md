## 1. Contract And Presentation

- [x] 1.1 Update `docs/next/session-time-travel.md` so the browser contract explains that legacy stale-disk empty baseline rows may be hidden from view while remaining on disk.
- [x] 1.2 Keep the local-history browser documentation and product copy aligned with the "hide legacy noise, preserve stored data" approach.

## 2. Browser Filtering

- [x] 2.1 Add a filtered local-history metadata view in `crates/lushtext-core/src/ui/window/local_history.rs` so legacy stale-disk empty baseline rows are omitted from the visible list.
- [x] 2.2 Keep the preview and action routing indexed from the filtered visible snapshot sequence rather than the raw stored metadata vector.
- [x] 2.3 Preserve legitimate empty snapshots that do not match the legacy stale-disk noise pattern.

## 3. Verification

- [x] 3.1 Add widget-test coverage with seeded legacy-pattern history so noisy empty baseline rows are hidden while useful rows remain visible.
- [x] 3.2 Re-run the focused local-history widget coverage after the browser filter lands.
