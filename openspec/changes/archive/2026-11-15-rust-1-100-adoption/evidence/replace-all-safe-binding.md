# In-tab Replace All on the safe binding (group 5)

Bindings: sourceview5 0.11.3, sourceview5-sys 0.11.3 (`Cargo.lock`).

## 5.3 probe: does GtkSourceView return a `GError`?

Yes. With regular expressions on, query `find`, and the replacement `bad\`
(a trailing backslash), `SearchContext::replace_all` returns `Err`, which the
widget test asserts directly (`context.replace_all("bad\\").is_err()`). The
document stays `find me, find me\n`, unmodified, with two occurrences. The
bar's `Err` arm therefore has a driver, and the widget test
`background_tab_layout::test_replace_all_with_an_invalid_regex_replacement_is_refused`
pins it through the bar as well (document unchanged, no panic, the `7 of 9`
sentinel replaced by the refreshed counter).

Command, through `scripts/heavy.sh --mem 10G`, CPUs 8-23:

```
scripts/run-widget-tests.sh --retries 0 -- background_tab_layout::test_replace_all
```

Result: `test result: ok. all tests passed` (3 tests: the no-match test, the
invalid-replacement test, and
`test_replace_all_right_after_a_paced_reattach_replaces_every_match`), clean
under the runner's warning scan; `widget-memory: peak 859 MiB, oom_kill 0`.

The first run of the no-match test failed on its own new assertion: it set the
query on the search settings directly, so the entry text stayed empty and
`policy::query_has_no_matches("", 0)` was, correctly, false. The test now types
the query into the entry, as a user does, and waits until it reaches the
settings before calling Replace All.

The direct `is_err()` assertion was added after the run above; the final
tree's `make test` (every widget shard, 0 FLAKY) runs it, and that run is the
evidence for task 5.4 (`background_tab_layout` is in the `editor-page` shard,
`minimap_markers` in the `window` shard, each in its own capped headless
session).
