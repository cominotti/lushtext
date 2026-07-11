## Context

LushText already centralizes chunked GTK text snapshots and background work. Four narrower paths remain: periodic local-history capture classifies eligibility from load-time file size and then copies the whole live buffer directly; opening Ctrl+F/Ctrl+H copies any selection into the query; decoded line-ending detection scans every byte scalarly despite the established `memchr` dependency; and every queued Markdown allocation repair traverses all embeds even when column width and embed membership are unchanged.

## Goals / Non-Goals

**Goals:**

- Classify periodic history capture from current buffer size and reject mixed/stale chunked snapshots.
- Cap in-editor search selection prefill at 1,024 characters without disturbing existing queries.
- Use `memchr2_iter` for CR/LF detection while preserving exact line-ending outcomes.
- Skip Markdown embed traversal when both effective column width and embed generation are unchanged.
- Add regression and micro-benchmark evidence for each focused optimization.

**Non-Goals:**

- Redesigning local-history retention, restoring, or save-boundary capture.
- Truncating user selections or changing editor buffer contents.
- Replacing readable scalar loops that are not byte-compatible hot paths.
- Removing the deferred Markdown idle/timed settling passes required by GTK geometry.

## Decisions

### Classify periodic history from live scalar state

At each periodic boundary, the editor derives a conservative current byte bound from `buffer.char_count() * 4`. Above the 50 MiB history ceiling, capture is unavailable; above the 10 MiB full-history threshold, periodic capture is skipped and the existing save-boundary-only policy applies. At or below 10 MiB, the shared snapshot helper decides direct versus chunked capture from the same live count.

Chunked capture records editor lifetime, file-path generation, periodic generation, and buffer edit generation. If any changes before capture completes, the mixed snapshot is discarded and normal scheduling decides whether to try later. The same facts are rechecked before worker persistence begins.

Alternatives considered:

- Trusting `file_size` was rejected because the live buffer can grow far beyond it.
- Always chunking periodic history was rejected because small direct snapshots are cheaper and already bounded.
- Persisting a snapshot assembled across concurrent edits was rejected because it may not represent any real document state.

### Cap search prefill before reading selection text

`MAX_SEARCH_SELECTION_PREFILL_CHARS` is 1,024, matching workspace search's query-sized policy. The editor compares selection offsets first. Only non-empty selections at or below the cap are copied into the search entry. Oversized selections leave the current query unchanged; the search bar still opens, focuses, and selects its existing query. This avoids allocating the large selection merely to truncate it.

### Use the existing byte-search dependency narrowly

`editor_io::detect_line_endings` will iterate CR and LF candidates through `memchr::memchr2_iter`, skipping the LF half of a counted CRLF pair. Tests retain exact LF, CRLF, CR, mixed, empty, and boundary semantics. Existing already-vectorized or parity-sensitive scans remain unchanged unless a benchmark demonstrates an equivalent clearer implementation.

### Cache Markdown width inputs, not only per-widget requests

Markdown preview state will track an embed-membership generation and the last processed `(effective_text_column_width, embed_generation)`. `refresh_code_block_widths` returns immediately when that tuple is unchanged. Rendering, clear, placeholder, or embed insertion/removal advances the embed generation; hidden/zero-width states do not overwrite the last valid tuple. Deferred idle and timed passes remain, so a later real allocation still repairs widths.

Alternatives considered:

- Removing deferred passes was rejected because child-anchor allocation settles late in supported GTK test/runtime paths.
- Caching only the root width was rejected because newly rendered embeds at the same width would never receive requests.

## Risks / Trade-offs

- [The four-byte live bound may skip periodic capture earlier for ASCII buffers] → Preserve save-boundary capture, document the conservative policy, and cover threshold behavior explicitly.
- [Editing during every long history snapshot can repeatedly invalidate it] → Reschedule through the existing five-minute policy without tight retries; saves still capture history.
- [Oversized selection no longer replaces an old query] → Keep the prior query selected and ready to overwrite, which is safer than silently truncating user text.
- [Width cache can hide a needed refresh] → Include embed generation, clear cache on lifecycle resets, and retain final deferred callbacks.

## Migration Plan

1. Add pure threshold/query-prefill helpers and unit tests.
2. Migrate periodic local-history capture to generation-aware direct/chunked outcomes.
3. Add pre-copy selection length gating and widget tests.
4. Replace line-ending scalar scanning with `memchr2_iter` and run existing encoding/property/benchmark coverage.
5. Add Markdown embed-generation width caching and geometry regression tests.
6. Rollback is per optimization; no persisted format changes are involved.

## Open Questions

None. Each optimization is deliberately local and can be reverted independently if measurement contradicts its value.
