# Threshold inventory

This is the committed inventory of every integer `const` and `static` in the
production crates' `src/` trees (`#[cfg(test)]` modules, `kani_proofs.rs` and
the `gtk-lush-axioms` probe parameters excluded). `make check-thresholds`
(`scripts/check-threshold-kinds.py`, part of `make check-policy`) fails when a
constant has no row, a row has no constant, a value differs from its row, an
in-scope constant is not a power of two without being `derived`, an
`external fact`, a `measurement record` or a recorded exception, or an
in-scope constant's rustdoc `Kind:` line is missing or disagrees with its row.

The rule is in `.agents/rules/rust.md` ("Thresholds are powers of two, rounded
the conservative way"); the classification and the measurements behind every
recorded exception come from `power-of-two-thresholds` (design D1–D10,
`openspec/changes/archive/*-power-of-two-thresholds/`).

## Kinds

In scope (the constant bounds work per dispatch, memory, retained entries or
retries, or decides a feature or tier from a size, length or count):

| Kind | Value |
|---|---|
| `cap` | rounded **down** to a power of two |
| `coverage window` | rounded **up** to a power of two |
| `retention tier` | rounded **up** (a smaller tier prunes data the user has) |
| `retry budget` | rounded **up** (a smaller budget reports a transient failure) |
| `text cap` | rounded down, and never below the longest shipped string |
| `policy tier` | a power of two aligned with its neighbours, shown in IEC units |
| `external fact` | exact, cited from its source (a toolkit or format fact) |
| `derived` | computed from named inputs, never rounded |
| `measurement record` | a measured value that decides nothing, exact |

A kind followed by `, exception` is a **recorded exception**: it keeps an exact
value because the power of two below it, and any measured one above it, cost a
measured user-visible loss — at least 0.05 % of a corpus of at least 2,000 files
that have the feature (design D2). Its rustdoc says
"Kind: <kind>; recorded exception" and gives the measurement.

The rustdoc of every in-scope constant carries a line starting `Kind: <kind>`.

Out of scope, each row names its class as `out: <class>`: `geometry` (sp, px),
`percent` (percentages, permille, the zoom grid), `priority` (GLib, CSS, mark),
`code` (exit codes, errno, hash constants), `time`, `factor` (multipliers,
divisors, units), `rate` (measured rates), `structural` (enum lengths, bucket
and worker counts, capacity hints), `presentation` (how many lines or rows a
surface shows), `identifier` (format versions, discriminants, sentinels),
`tool` (proof tools, fixtures, fuzzing, test helpers and Kani bounds).

## Maintaining it

A new constant fails the check until it has a row. Classify it, round it, give
it a `Kind:` line, then add the row (`scripts/check-threshold-kinds.py --write`
regenerates the table, keeping the kind and reason of every existing row and
marking new ones `UNCLASSIFIED`; review the diff).

<!-- inventory -->

### `crates/cargo-gtk-proof/src/artifacts.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/artifacts.rs` | `MAX_WRITTEN_JSON_BYTES` | 8,388,608 | cap | power of two |

### `crates/cargo-gtk-proof/src/automation.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/automation.rs` | `ARTIFACT_TEXT_LIMIT` | 2,048 | presentation cap | up from 1,200: inside the unchanged 8 MiB JSON caps (D12, task 9.4) |

### `crates/cargo-gtk-proof/src/lib.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/lib.rs` | `MAX_ARTIFACT_JSON_BYTES` | 8,388,608 | cap | power of two |

### `crates/cargo-gtk-proof/src/live.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/live.rs` | `FINAL_GEOMETRY_SAMPLE_COUNT` | 3 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/cargo-gtk-proof/src/live.rs` | `DEFAULT_ANIMATION_STREAM_FRAME_COUNT` | 48 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/cargo-gtk-proof/src/live.rs` | `DEFAULT_ANIMATION_MAX_SAMPLE_SKEW_MS` | 80 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/cargo-gtk-proof/src/live.rs` | `REVEAL_WAIT_TIMEOUT_MS` | 30,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/cargo-gtk-proof/src/live.rs` | `FIRST_FRAME_COMPOSE_SLACK_MS` | 15,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/cargo-gtk-proof/src/model.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/model.rs` | `SUPPORTED_SCHEMA_VERSION` | 1 | out: identifier | format version, discriminant or sentinel (D1) |

### `crates/cargo-gtk-proof/src/png.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/png.rs` | `MAX_COMPRESSED_PNG_BYTES` | 33,554,432 | cap | power of two |
| `crates/cargo-gtk-proof/src/png.rs` | `MAX_DECODED_PNG_BYTES` | 67,108,864 | cap | power of two |
| `crates/cargo-gtk-proof/src/png.rs` | `MAX_PNG_PIXELS` | 16,777,216 | cap | power of two |
| `crates/cargo-gtk-proof/src/png.rs` | `FROZEN_WORKSPACE_ROW_INK_OFFSET` | 25 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/cargo-gtk-proof/src/png.rs` | `ROW_INK_BACKGROUND_DISTANCE` | 40 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/cargo-gtk-proof/src/process.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/process.rs` | `MAX_LOG_BYTES` | 65,536 | cap | power of two |

### `crates/cargo-gtk-proof/src/reveal_fixture.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/reveal_fixture.rs` | `LARGE_DIRECTORY_ENTRIES` | 400 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/cargo-gtk-proof/src/reveal_fixture.rs` | `NESTED_DIRECTORY_ENTRIES` | 300 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/cargo-gtk-proof/src/runner.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/runner.rs` | `OPEN_POPOVER_LIST_CAP_TOLERANCE` | 12 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/cargo-gtk-proof/src/runner.rs` | `OPEN_POPOVER_VIEWPORT_SHADOW_TOLERANCE` | 8 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/cargo-gtk-proof/src/runner.rs` | `REVEALED_NEIGHBOUR_LIMIT` | 8 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/cargo-gtk-proof/src/warnings.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/cargo-gtk-proof/src/warnings.rs` | `WARNING_TEXT_LIMIT` | 2,048 | presentation cap | up from 1,200: inside the unchanged 8 MiB JSON caps (D12, task 9.4) |

### `crates/gtk-lush-adoption-lab/src/widgets_page.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/gtk-lush-adoption-lab/src/widgets_page.rs` | `SLICE_DEMO_ROWS` | 1,000 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/gtk-lush/proof-harness/src/lib.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/gtk-lush/proof-harness/src/lib.rs` | `TEST_FAILURE_EXIT_CODE` | 101 | out: code | exit code, errno or hash constant (D1) |
| `crates/gtk-lush/proof-harness/src/lib.rs` | `UNSUPPORTED_HOST_EXIT_CODE` | 77 | out: code | exit code, errno or hash constant (D1) |
| `crates/gtk-lush/proof-harness/src/lib.rs` | `DEFAULT_TEST_ATTEMPTS` | 2 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/gtk-lush/proof-spine/src/lib.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/gtk-lush/proof-spine/src/lib.rs` | `CURRENT_SCHEMA_VERSION` | 1 | out: identifier | format version, discriminant or sentinel (D1) |

### `crates/gtk-lush/tasks/src/lib.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/gtk-lush/tasks/src/lib.rs` | `DEFAULT_MAX_CONCURRENT_SPAWNS` | 8 | cap | power of two |
| `crates/gtk-lush/tasks/src/lib.rs` | `COMPLETION_PRIORITY_VALUE` | 124 | out: priority | GLib, CSS, mark or gutter priority (D1) |
| `crates/gtk-lush/tasks/src/lib.rs` | `COMPLETIONS_BEFORE_YIELD` | 8 | cap | power of two |

### `crates/lushtext-core/src/fuzzing.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/fuzzing.rs` | `FUZZ_MAX_INPUT_LEN` | 4,096 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/fuzzing.rs` | `MARKDOWN_FUZZ_MAX_INPUT_LEN` | 131,072 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/fuzzing.rs` | `OPERATION_SCRIPT_MAX_INPUT_LEN` | 4,096 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/fuzzing.rs` | `OPERATION_SCRIPT_MAX_OPERATIONS` | 32 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/fuzzing.rs` | `OPERATION_SCRIPT_MAX_TEXT_BYTES` | 256 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/fuzzing.rs` | `OPERATION_SCRIPT_MAX_PATH_BYTES` | 64 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/fuzzing.rs` | `OPERATION_SCRIPT_MAX_FILES` | 3 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/lushtext-core/src/model/attention_refresh.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/attention_refresh.rs` | `ATTENTION_REFRESH_DURATION_MULTIPLIER` | 10 | out: factor | factor, divisor or unit (D1) |

### `crates/lushtext-core/src/model/automation.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/automation.rs` | `AUTOMATION_WORKFLOW_EVENT_LIMIT` | 128 | presentation cap | up from 128: events an automation client receives (D12) |

### `crates/lushtext-core/src/model/buffer_replacement.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/buffer_replacement.rs` | `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES` | 524,288 | cap | 512 KiB: laid out within 6.2 ms of main at 512-704 KiB (D8, task 5.3) |
| `crates/lushtext-core/src/model/buffer_replacement.rs` | `REPLACEMENT_CLEAR_SLICE_CHARS` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/model/buffer_replacement.rs` | `REPLACEMENT_INSERT_SLICE_BYTES` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/model/buffer_replacement.rs` | `V_SLICE` | 16,384 | cap | 16,384: lane dispatches within main's, laid out no slower (D8, task 5.2) |

### `crates/lushtext-core/src/model/construction_spans.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/construction_spans.rs` | `MAX_CONSTRUCTION_STAGES` | 64 | coverage window | rounded up (D5) |

### `crates/lushtext-core/src/model/content_search.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/content_search.rs` | `MAX_SEARCH_MATCH_LINE_BYTES` | 4,096 | presentation cap | up from 4,096: past it a match line is cut (D12) |
| `crates/lushtext-core/src/model/content_search.rs` | `MAX_REPLACE_PREVIEW_ROWS` | 16,384 | presentation cap | up from 10,000: M3, preview worst dispatch 27.8-29.2 ms vs main 26.0-28.6 ms, 64 MiB budget unchanged (D12) |
| `crates/lushtext-core/src/model/content_search.rs` | `MAX_REPLACE_PREVIEW_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it Replace Preview omits rows (D12) |
| `crates/lushtext-core/src/model/content_search.rs` | `MAX_REPLACE_DIAGNOSTIC_ENTRIES` | 32 | presentation cap | up from 32: diagnostics a user reads after Replace All (D12) |
| `crates/lushtext-core/src/model/content_search.rs` | `MAX_REPLACE_DIAGNOSTIC_BYTES` | 32,768 | presentation cap | up from 32,768: diagnostic text a user reads (D12) |

### `crates/lushtext-core/src/model/document_scale.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/document_scale.rs` | `S_SYNTAX` | 1,048,576 | presentation cap | up from 1,048,576: plain-text highlighting turns off above it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::HTML` | 47,104 | presentation cap, exception | up from 47,104, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::MARKDOWN` | 327,680 | presentation cap, exception | up from 327,680, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::RUST` | 188,416 | presentation cap, exception | up from 188,416, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::JSON` | 24,576 | presentation cap, exception | up from 24,576, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::C` | 86,016 | presentation cap, exception | up from 86,016, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::PYTHON` | 49,152 | presentation cap, exception | up from 49,152, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::JAVASCRIPT` | 18,432 | presentation cap, exception | up from 18,432, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::TYPESCRIPT` | 10,752 | presentation cap, exception | up from 10,752, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::YAML` | 147,456 | presentation cap, exception | up from 147,456, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::XML` | 135,168 | presentation cap, exception | up from 135,168, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::CSS` | 37,888 | presentation cap, exception | up from 37,888, kept: A69's sweep derived it as the largest passing CSS size, so 64 KiB fails (cited; D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `attach_threshold::SHELL` | 39,936 | presentation cap, exception | up from 39,936, kept: A69's sweep: the power of two above fails its attach bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `typing_threshold::HTML` | 16,384 | presentation cap | up from 16,384: highlighting turns off above it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `typing_threshold::JSON` | 20,480 | presentation cap, exception | up from 20,480, kept: A69's sweep: the power of two above fails the typing bound (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `MODEL_MARKDOWN_CAP` | 524,288 | policy tier | power of two |
| `crates/lushtext-core/src/model/document_scale.rs` | `S_SYNTAX_OTHER` | 48,128 | presentation cap, exception | up from 48,128, kept: A69's sweep (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_SYNTAX` | 2,000 | presentation cap, exception | up from 2,000, kept: 2,048 breaks MAX_SYNTAX_PARAGRAPH_BYTES <= HIGHLIGHT_CHUNK_BYTES (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `S_MINIMAP` | 10,000,000 | presentation cap, exception | up from 10,000,000, kept: M10, a 16 MiB open with the minimap 37-38 ms vs main 23-27 ms and one settle past the 10 s cap (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_MINIMAP` | 22,000 | presentation cap, exception | up from 22,000, kept: M14 measured the minimap-on background dispatch at 32,768 13.4 ms worse than main's at 22,000 (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_INVISIBLES` | 50,000 | presentation cap, exception | up from 50,000, kept: it must not exceed L_PROTECT_EDIT (Protected View has invisibles off, C2 D10; asserted) (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_SEARCH_HIGHLIGHT` | 16,384 | presentation cap | up from 10,000: M14, find worst 9.6-10.0 ms vs main 6.5-8.6 ms (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_WRAP_EDIT` | 8,192 | presentation cap | up from 8,192: word wrap turns off above it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_PROTECT` | 512 | presentation cap | up from 500: Protected View length gate (D7) (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_EDIT_STALL` | 3,500 | measurement record | measured, decides nothing (D2) |
| `crates/lushtext-core/src/model/document_scale.rs` | `L_PROTECT_EDIT` | 50,000 | presentation cap, exception | up from 50,000, kept: the maintainer's editability contract; C5 measured the keystroke cost rising past it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `N_VISIBLE` | 262,144 | cap | rounded down (D4) |
| `crates/lushtext-core/src/model/document_scale.rs` | `V_BATCH` | 65,536 | presentation cap | up from 56,000: N_DENSE 256 (main 250); M11, folded page worst 10.9-15.0 ms vs main 11.3-19.2 ms at 100 % and 50 % zoom (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `R_BATCH` | 256 | coverage window | 224 rounded up to 256 (D5) |
| `crates/lushtext-core/src/model/document_scale.rs` | `UNDO_TOTAL_BYTES` | 67,108,864 | presentation cap | up from 50,000,000: undo turns off above it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `HISTORY_SAVE_ONLY_BYTES` | 16,777,216 | presentation cap | up from 10,000,000: M10, periodic capture at the tier worst dispatch under 1 ms (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `HISTORY_UNAVAILABLE_BYTES` | 67,108,864 | presentation cap | up from 50,000,000: local history turns off above it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `REFUSE_TO_OPEN` | 536,870,912 | presentation cap | up from 500,000,000: opening is refused above it (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `MULTIPLIER_WITH_UNDO` | 3 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/model/document_scale.rs` | `MULTIPLIER_WITHOUT_UNDO` | 2 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/model/document_scale.rs` | `PRESENTATION_MASK` | `expr: ScaleFeature::Undo.bit() - 1` | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/model/document_scale.rs` | `byte_tier_reference::DISABLE_SYNTAX_HIGHLIGHTING` | 16,777,216 | presentation cap | up from 10,000,000: independent mirror of HISTORY_SAVE_ONLY_BYTES (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `byte_tier_reference::DISABLE_UNDO_HISTORY` | 67,108,864 | presentation cap | up from 50,000,000: reference mirror of UNDO_TOTAL_BYTES (D12) |
| `crates/lushtext-core/src/model/document_scale.rs` | `byte_tier_reference::REFUSE_TO_OPEN` | 536,870,912 | presentation cap | up from 500,000,000: reference mirror of REFUSE_TO_OPEN (D12) |

### `crates/lushtext-core/src/model/draft.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/draft.rs` | `UNPROVEN_BACKING_MTIME_SECS` | 18,446,744,073,709,551,615 | out: identifier | format version, discriminant or sentinel (D1) |

### `crates/lushtext-core/src/model/editor_memory.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/editor_memory.rs` | `EDITOR_MEMORY_UPPER_BUDGET_BYTES` | 268,435,456 | cap | power of two |
| `crates/lushtext-core/src/model/editor_memory.rs` | `EDITOR_MEMORY_LOWER_WATER_BYTES` | `expr: EDITOR_MEMORY_UPPER_BUDGET_BYTES.saturating_mul(90) / 100` | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/editor_memory.rs` | `EVICTED_EDITOR_BOOKKEEPING_BYTES` | 4,096 | coverage window | power of two, rounded up |
| `crates/lushtext-core/src/model/editor_memory.rs` | `PROTECTED_BOUNDARY_BYTES` | 128 | coverage window | power of two, rounded up |

### `crates/lushtext-core/src/model/file_load.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/file_load.rs` | `TRANSIENT_LOAD_SHARED_BUDGET_BYTES` | 268,435,456 | cap | power of two |
| `crates/lushtext-core/src/model/file_load.rs` | `TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES` | 1,048,576 | cap | power of two |
| `crates/lushtext-core/src/model/file_load.rs` | `TRANSIENT_LOAD_SOURCE_MULTIPLIER` | 8 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/model/file_load.rs` | `LIVE_RESIDENCY_BYTES_PER_CHAR` | 4 | external fact | exact, cited from its source |
| `crates/lushtext-core/src/model/file_load.rs` | `DECODED_BODY_SOURCE_MULTIPLIER` | 3 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/model/file_load.rs` | `SYNCHRONOUS_INSTALL_THRESHOLD_BYTES` | 524,288 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/file_load.rs` | `INSTALL_SLICE_BYTES` | 262,144 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/file_load.rs` | `MAX_ADMITTED_FILE_LOADS` | 8 | cap | power of two |
| `crates/lushtext-core/src/model/file_load.rs` | `MAX_CONSECUTIVE_ACTIVE_BYPASSES` | 2 | cap | power of two |

### `crates/lushtext-core/src/model/held_input.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/held_input.rs` | `HELD_TEXT_CAP_BYTES` | 4,096 | presentation cap | up from 4,096: typed text kept during install (D12) |

### `crates/lushtext-core/src/model/highlight_chunking.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/highlight_chunking.rs` | `HIGHLIGHT_CHUNK_BYTES` | 8,192 | cap | power of two |
| `crates/lushtext-core/src/model/highlight_chunking.rs` | `MAX_SYNTAX_PARAGRAPH_BYTES` | 8,003 | derived | computed from named inputs; not rounded (D2) |

### `crates/lushtext-core/src/model/paragraph_boundary.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `FALSE_SEPARATOR_LEADS #[cfg(not(kani))]` | 4 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `DELIMITER_SEARCH_CHUNK #[cfg(not(kani))]` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `PROFILE_THRESHOLD_COUNT` | 6 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `MAX_PROFILED_OFFSETS #[cfg(not(kani))]` | 256 | cap | power of two |
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `MAX_DENSITY_WINDOW #[cfg(not(kani))]` | 256 | cap | power of two |
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `MAX_PROFILED_OFFSETS #[cfg(kani)]` | 4 | out: tool | Kani harness bound (D1) |
| `crates/lushtext-core/src/model/paragraph_boundary.rs` | `MAX_DENSITY_WINDOW #[cfg(kani)]` | 4 | out: tool | Kani harness bound (D1) |

### `crates/lushtext-core/src/model/paragraph_wrap.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/paragraph_wrap.rs` | `REWRAP_MARGIN` | 1,024 | coverage window | power of two, rounded up |

### `crates/lushtext-core/src/model/protected_fold.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/protected_fold.rs` | `N_HEAD` | 10,000 | presentation cap, exception | up from 10,000, kept: C2's sweep, 16.1 ms at 14,000 wrap off and 20.7 ms at 20,000 wrap on, so 16,384 fails (cited; D12) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `A28_MEASURED_ADVANCE_UNITS` | 9,011 | external fact | exact, cited from its source |
| `crates/lushtext-core/src/model/protected_fold.rs` | `A28_MAX_ZOOM_PERCENT` | 400 | out: percent | percentage, permille or zoom percent (D1) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `A28_SAFE_CHARS` | 59,579 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `R_FOLD` | 256 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `R_FOLD_MIN_ZOOM_PERCENT` | 50 | out: percent | percentage, permille or zoom percent (D1) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `N_DENSE` | 256 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `HEAD_CAP` | `expr: min_u64(min_u64(N_VISIBLE, N_HEAD), A28_SAFE_CHARS)` | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `N_INSPECT` | 512 | presentation cap | up from 500: characters the Line Inspector shows (D7) (D12) |
| `crates/lushtext-core/src/model/protected_fold.rs` | `MAX_R_FOLD` | 256 | cap | power of two |
| `crates/lushtext-core/src/model/protected_fold.rs` | `FoldRange::RETAINED_BYTES` | `expr: std::mem::size_of::<Self>() as u64` | derived | computed from named inputs; not rounded (D2) |

### `crates/lushtext-core/src/model/protected_install.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/protected_install.rs` | `MAX_UTF8_SCALAR_BYTES` | 4 | external fact | exact, cited from its source |
| `crates/lushtext-core/src/model/protected_install.rs` | `MIN_TAIL_SLICE_BYTES` | 4 | external fact | the longest UTF-8 scalar |
| `crates/lushtext-core/src/model/protected_install.rs` | `MAX_SLICE_BYTES` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/model/protected_install.rs` | `B_SETTLE_US` | `expr: super::responsiveness_budget::B_SETTLE_US` | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/protected_install.rs` | `PROTECTED_TAIL_COST_NS_PER_KIB` | 26,000 | out: rate | measured rate (D1) |
| `crates/lushtext-core/src/model/protected_install.rs` | `PROTECTED_CLEAR_COST_NS_PER_KIB` | 26,000 | out: rate | measured rate (D1) |
| `crates/lushtext-core/src/model/protected_install.rs` | `PROTECTED_SLICE_MARGIN_PERCENT` | 100 | out: percent | percentage, permille or zoom percent (D1) |
| `crates/lushtext-core/src/model/protected_install.rs` | `PROTECTED_TAIL_SLICE_BYTES` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/model/protected_install.rs` | `PROTECTED_CLEAR_SLICE_BYTES` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/model/protected_install.rs` | `PROTECTED_TAIL_SCAN_BYTES` | 33,554,432 | cap | power of two |

### `crates/lushtext-core/src/model/reanalysis_pacing.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/reanalysis_pacing.rs` | `REANALYSIS_ALLOWANCE_US` | `expr: B_SETTLE_US` | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/reanalysis_pacing.rs` | `STEP_TARGET_US` | 2,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/reanalysis_pacing.rs` | `FIRST_STEP_CHARS` | 512 | coverage window | power of two, rounded up |
| `crates/lushtext-core/src/model/reanalysis_pacing.rs` | `MIN_STEP_CHARS` | 128 | cap | power of two |
| `crates/lushtext-core/src/model/reanalysis_pacing.rs` | `MAX_STEP_CHARS` | 1,048,576 | cap | power of two |
| `crates/lushtext-core/src/model/reanalysis_pacing.rs` | `MAX_STEPS` | 256 | cap | power of two |

### `crates/lushtext-core/src/model/recent_document.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/recent_document.rs` | `age_label::MINUTE` | 60 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/recent_document.rs` | `age_label::HOUR` | 3,600 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/recent_document.rs` | `age_label::DAY` | 86,400 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/model/responsiveness_budget.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `B_INTERACT_US` | 8,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `B_SETTLE_US` | 16,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `REFERENCE_FRAME_US` | 8,333 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `INPUT_TO_FRAME_FRAMES` | 2 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `INPUT_TO_FRAME_US` | 16,666 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `SCROLL_P95_FRAME_GAP_US` | 8,333 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `SCROLL_FRAME_GAP_PERCENTILE` | 95 | out: percent | percentage, permille or zoom percent (D1) |
| `crates/lushtext-core/src/model/responsiveness_budget.rs` | `HISTOGRAM_BUCKETS` | 24 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |

### `crates/lushtext-core/src/model/sidecar_identity.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/sidecar_identity.rs` | `stable_bytes_hash::FNV_OFFSET` | 14,695,981,039,346,656,037 | out: code | exit code, errno or hash constant (D1) |
| `crates/lushtext-core/src/model/sidecar_identity.rs` | `stable_bytes_hash::FNV_PRIME` | 1,099,511,628,211 | out: code | exit code, errno or hash constant (D1) |

### `crates/lushtext-core/src/model/size_units.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/size_units.rs` | `KIB` | 1,024 | out: factor | factor, divisor or unit (D1) |

### `crates/lushtext-core/src/model/surrounding_text.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/surrounding_text.rs` | `SURROUNDING_WINDOW_CHARS` | 1,024 | presentation cap | up from 1,024: context an input method receives (D12) |
| `crates/lushtext-core/src/model/surrounding_text.rs` | `SURROUNDING_MAX_SELECTION_CHARS` | 8,192 | presentation cap | up from 8,192: selection an input method receives (D12) |

### `crates/lushtext-core/src/model/workspace_search.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/workspace_search.rs` | `WORKSPACE_SEARCH_FALLBACK_ENTRY_LIMIT` | 65,536 | presentation cap | up from 65,536: past it the search is incomplete (D12) |
| `crates/lushtext-core/src/model/workspace_search.rs` | `WORKSPACE_SEARCH_FALLBACK_PATH_BYTE_LIMIT` | 8,388,608 | presentation cap | up from 8,388,608: past it the search is incomplete (D12) |

### `crates/lushtext-core/src/model/workspace_visibility.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/model/workspace_visibility.rs` | `SNAPSHOT_EXCLUDED_NAMES_LIMIT` | 64 | presentation cap | up from 64: names an automation snapshot lists (D12) |

### `crates/lushtext-core/src/services/app_data_leftovers.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/app_data_leftovers.rs` | `MAX_LINEAGES_PER_PASS` | 1,024 | cap | power of two |
| `crates/lushtext-core/src/services/app_data_leftovers.rs` | `MAX_BACKUP_RUNS_PER_PASS` | 256 | cap | power of two |

### `crates/lushtext-core/src/services/bookmark_excerpt.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/bookmark_excerpt.rs` | `BOOKMARK_EXCERPT_CONTEXT_BEFORE_LINES` | 3 | out: presentation | presentation shape: rows or lines a surface shows (D1) |
| `crates/lushtext-core/src/services/bookmark_excerpt.rs` | `BOOKMARK_EXCERPT_CONTEXT_AFTER_LINES` | 7 | out: presentation | presentation shape: rows or lines a surface shows (D1) |
| `crates/lushtext-core/src/services/bookmark_excerpt.rs` | `BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT` | 1,048,576 | presentation cap | up from 1,048,576: past it a bookmark has no excerpt (D12) |
| `crates/lushtext-core/src/services/bookmark_excerpt.rs` | `BOOKMARK_EXCERPT_SCAN_LINE_LIMIT` | 32,768 | presentation cap | up from 20,000: M12, 28.8-28.9 MiB peak vs main 28.1-28.5, under 1 ms; the 1 MiB byte limit bounds it (D12) |
| `crates/lushtext-core/src/services/bookmark_excerpt.rs` | `BOOKMARK_EXCERPT_LINE_CHAR_LIMIT` | 4,096 | presentation cap | up from 4,096: characters an excerpt line shows (D12) |
| `crates/lushtext-core/src/services/bookmark_excerpt.rs` | `BOOKMARK_EXCERPT_CANCELLATION_CHECK_LINES` | 1,024 | cap | power of two |

### `crates/lushtext-core/src/services/content_search/replace.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/content_search/replace.rs` | `MAX_REPLACE_FILE_BYTES` | 16,777,216 | presentation cap | up from 10,485,760: M9, one-file peak growth 33.9 vs 21.6 MiB (ratio 1.57); the 64 MiB aggregate unchanged (D12) |
| `crates/lushtext-core/src/services/content_search/replace.rs` | `MAX_REPLACE_UNDO_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it Replace All skips files (D12) |
| `crates/lushtext-core/src/services/content_search/replace.rs` | `MAX_REPLACE_UNDO_RETAINED_BYTES` | 67,108,864 | cap | power of two |

### `crates/lushtext-core/src/services/content_search/search.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/content_search/search.rs` | `RESULT_CAP` | 16,384 | presentation cap | up from 10,000: M2, search worst 22-26 ms vs main 20-23 ms (the first-results reveal), 256 events per turn (D12) |

### `crates/lushtext-core/src/services/draft_service.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_AUTOMATIC_DRAFT_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it unsaved work has no draft (D12) |
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_EAGER_DRAFT_PRELOAD_BYTES` | 67,108,864 | cap | power of two |
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_MANIFEST_REPAIR_DRAFT_SCAN` | 2,048 | cap | power of two |
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_MANIFEST_REPAIR_PAGE_ENTRIES` | 256 | cap | power of two |
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_MANIFEST_REPAIR_METADATA_BYTES` | 524,288 | cap | power of two |
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_MANIFEST_REPAIR_DIAGNOSTICS` | 4 | presentation cap | up from 4: diagnostics reported from a repair (task 9.1) (D12) |
| `crates/lushtext-core/src/services/draft_service.rs` | `MANIFEST_REPAIR_ENTRY_OVERHEAD_BYTES` | 128 | coverage window | rounded up (D5) |
| `crates/lushtext-core/src/services/draft_service.rs` | `MANIFEST_REPAIR_AUXILIARY_ENTRY_ALLOWANCE` | 1 | coverage window | power of two, rounded up |
| `crates/lushtext-core/src/services/draft_service.rs` | `MAX_ORPHAN_CLEANUP_DRAFT_SCAN` | 2,048 | cap | power of two |

### `crates/lushtext-core/src/services/draft_service/set_aside.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/draft_service/set_aside.rs` | `MAX_SET_ASIDE_NAME_ATTEMPTS` | 64 | retry budget | power of two |
| `crates/lushtext-core/src/services/draft_service/set_aside.rs` | `MAX_LISTED_SET_ASIDE_BODIES` | 256 | presentation cap | up from 256: bodies the review lists (D12) |
| `crates/lushtext-core/src/services/draft_service/set_aside.rs` | `MAX_SET_ASIDE_SCAN_ENTRIES` | 16,384 | presentation cap | up from 10,000: M12, 27.6-28.0 MiB, 23 ms (main 27.4-27.9 MiB, 14 ms) (D12) |

### `crates/lushtext-core/src/services/draft_service/set_aside_retention.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/draft_service/set_aside_retention.rs` | `SOFT_BOUND_BODIES` | 128 | policy tier | 100 to 128 (D6) |
| `crates/lushtext-core/src/services/draft_service/set_aside_retention.rs` | `SOFT_BOUND_BYTES` | 268,435,456 | policy tier | power of two |
| `crates/lushtext-core/src/services/draft_service/set_aside_retention.rs` | `MATERIAL_GROWTH_PERCENT` | 25 | out: percent | percentage, permille or zoom percent (D1) |

### `crates/lushtext-core/src/services/editor_io.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/editor_io.rs` | `MAX_LOSSY_PREVIEW_ISSUES` | 8 | presentation cap | up from 8: issues the lossy-decode preview lists (D12) |
| `crates/lushtext-core/src/services/editor_io.rs` | `LOAD_PROCESSING_CHUNK_BYTES` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/services/editor_io.rs` | `DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES` | 1,048,576 | cap | power of two |
| `crates/lushtext-core/src/services/editor_io.rs` | `ROUND_TRIP_CHUNK_BYTES` | 65,536 | cap | power of two |

### `crates/lushtext-core/src/services/file_peek.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/file_peek.rs` | `PEEK_SAMPLE_BYTE_LIMIT` | 16,384 | presentation cap | up from 16,384: bytes the file peek shows (D12) |
| `crates/lushtext-core/src/services/file_peek.rs` | `PEEK_SAMPLE_LINE_LIMIT` | 60 | out: presentation | presentation shape: rows or lines a surface shows (D1) |

### `crates/lushtext-core/src/services/filesystem/sys.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/filesystem/sys.rs` | `READ_CHUNK_BYTES` | 65,536 | cap | power of two |

### `crates/lushtext-core/src/services/filesystem/write_protocol.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/filesystem/write_protocol.rs` | `MAX_TEMP_NAME_ATTEMPTS` | 8 | cap | power of two |

### `crates/lushtext-core/src/services/format_upgrade/backup.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/format_upgrade/backup.rs` | `MAX_BACKUP_DIR_ATTEMPTS` | 64 | retry budget | power of two |
| `crates/lushtext-core/src/services/format_upgrade/backup.rs` | `MAX_BACKUP_ITEM_BYTES` | 16,777,216 | presentation cap | up from 16,777,216: past it an item is not backed up (D12) |

### `crates/lushtext-core/src/services/format_upgrade/diagnostics.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/format_upgrade/diagnostics.rs` | `SIDECAR_SCAN_MAX_ENTRIES` | 16,384 | presentation cap | up from 10,000: M12, preflight 34.4-35.3 MiB, 107 ms (main 31.5-32.3, 64 ms) (D12) |
| `crates/lushtext-core/src/services/format_upgrade/diagnostics.rs` | `LOCAL_HISTORY_SCAN_MAX_LINEAGES` | 16,384 | presentation cap | up from 10,000: M12, with the sidecar scan (D12) |
| `crates/lushtext-core/src/services/format_upgrade/diagnostics.rs` | `REPLACE_JOURNAL_SCAN_MAX_ENTRIES` | 16,384 | presentation cap | up from 10,000: equal to JOURNAL_SCAN_MAX_ENTRIES (asserted); M12 (D12) |
| `crates/lushtext-core/src/services/format_upgrade/diagnostics.rs` | `DRAFT_BODY_SCAN_MAX_ENTRIES` | 2,048 | cap | power of two |

### `crates/lushtext-core/src/services/json_format.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/json_format.rs` | `SUPPORTED_JSON_VERSION` | 1 | out: identifier | format version, discriminant or sentinel (D1) |

### `crates/lushtext-core/src/services/local_history_service.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/local_history_service.rs` | `PER_DOCUMENT_SNAPSHOT_CAP` | 64 | retention tier | rounded up (D6) |
| `crates/lushtext-core/src/services/local_history_service.rs` | `GLOBAL_SNAPSHOT_CAP` | 256 | retention tier | rounded up (D6) |
| `crates/lushtext-core/src/services/local_history_service.rs` | `DEFAULT_RECONCILE_MAX_LINEAGES` | 512 | cap | power of two |
| `crates/lushtext-core/src/services/local_history_service.rs` | `DEFAULT_RECONCILE_MAX_MILLIS` | 50 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/services/local_history_service.rs` | `MAX_INDEX_REPAIR_SNAPSHOT_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it a damaged lineage stays unavailable (D12) |

### `crates/lushtext-core/src/services/markdown_render.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_SOURCE_BYTES` | 4,194,304 | presentation cap | up from 4,194,304: past it the preview is limited (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_EVENTS` | 65,536 | presentation cap | up from 50,000: M13, planning 35.1-35.4 MiB vs 33.1-33.5, projection worst 0.6-2.3 ms (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_STRUCTURE_DEPTH` | 128 | presentation cap | up from 128: deeper structure is omitted from the preview (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_EMBED_DESCRIPTORS` | 256 | presentation cap | up from 256: embeds past it are omitted (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_RETAINED_BYTES` | 8,388,608 | presentation cap | up from 8,388,608: past it the preview is limited (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MARKDOWN_EVENTS_PER_PROJECTION_SLICE` | 256 | cap | power of two |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MARKDOWN_BYTES_PER_PROJECTION_SLICE` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_CARRIED_EMBED_BYTES` | 65,536 | presentation cap | up from 65,536: a larger embed falls back (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_CARRIED_TABLE_CELLS` | 1,024 | presentation cap | up from 1,000: M13, table build worst 10.3-12.3 ms vs main 10.8-11.4 ms (D12) |
| `crates/lushtext-core/src/services/markdown_render.rs` | `MAX_MARKDOWN_PLACEHOLDER_WIDGETS` | 64 | presentation cap | up from 64: placeholders past it are omitted (D12) |

### `crates/lushtext-core/src/services/migration_ledger.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/migration_ledger.rs` | `MAX_MIGRATION_ATTEMPTS` | 4 | retry budget | rounded up (D6) |

### `crates/lushtext-core/src/services/palette/fuzzy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/palette/fuzzy.rs` | `PALETTE_CANCEL_CHECK_INTERVAL` | 256 | cap | power of two |

### `crates/lushtext-core/src/services/palette/index.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/palette/index.rs` | `MAX_SCAN_DEPTH` | 64 | presentation cap | up from 64: deeper files are not indexed (D12) |
| `crates/lushtext-core/src/services/palette/index.rs` | `MAX_INDEXED_FILES` | 131,072 | presentation cap | up from 100,000: M15, 83.4-84.1 MiB vs main 68.3-68.5 (growth ratio 1.375), 546-555 ms vs 404-419 ms (D12) |
| `crates/lushtext-core/src/services/palette/index.rs` | `MAX_INDEXED_DIRECTORIES` | 131,072 | presentation cap | up from 100,000: with MAX_INDEXED_FILES (M15, D12) |
| `crates/lushtext-core/src/services/palette/index.rs` | `MAX_FILE_INDEX_RETAINED_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it the index is truncated (D12) |
| `crates/lushtext-core/src/services/palette/index.rs` | `MAX_FILE_INDEX_BUILD_RETAINED_BYTES` | 134,217,728 | presentation cap | up from 134,217,728: past it the index is truncated (D12) |

### `crates/lushtext-core/src/services/palette/notes.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_NOTE_FUZZY_SCORE_BYTES` | 4,096 | presentation cap | up from 4,096: the prefix nucleo scores orders results (D12) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_NOTE_FIELD_FUZZY_SCORE` | 65,535 | external fact | exact, cited from its source |
| `crates/lushtext-core/src/services/palette/notes.rs` | `NOTE_TEXT_CANCEL_CHECK_INTERVAL` | 1,024 | cap | power of two |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_PALETTE_NOTE_ENTRIES` | 16,384 | presentation cap | up from 10,000: M12, 85.9-86.1 MiB vs main 62.5-62.8 (growth ratio 1.67), 17 ms (D12) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_PALETTE_NOTE_TEXT_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it palette notes are truncated (D12) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_PALETTE_NOTE_RETAINED_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it palette notes are truncated (D12) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES` | 8,388,608 | presentation cap | up from 8,388,608: past it palette notes are truncated (D12) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_PALETTE_NOTE_CONSTRUCTION_BYTES` | 100,663,296 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `NOTE_SIDECAR_MODEL_EXPANSION_MULTIPLIER` | 4 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/services/palette/notes.rs` | `NOTE_SIDECAR_DIAGNOSTIC_RESERVATION_BYTES` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/services/palette/notes.rs` | `MAX_PALETTE_NOTE_DIAGNOSTICS` | 1,024 | presentation cap | up from 1,024: diagnostics the user is shown (D12) |

### `crates/lushtext-core/src/services/read_fault.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/read_fault.rs` | `EACCES #[cfg(feature="smoke-read-faults")]` | 13 | out: code | exit code, errno or hash constant (D1) |

### `crates/lushtext-core/src/services/recent_documents.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/recent_documents.rs` | `MAX_RECENTS` | 256 | retention tier | rounded up (D6) |
| `crates/lushtext-core/src/services/recent_documents.rs` | `MAX_RECENT_DOCUMENTS_BYTES` | 4,194,304 | coverage window | loads a full store of worst-case entries (D6) |
| `crates/lushtext-core/src/services/recent_documents.rs` | `MAX_RECENT_LOAD_CANDIDATES` | 512 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/services/recent_documents.rs` | `MAX_RECENT_LOAD_DIAGNOSTICS` | 32 | presentation cap | up from 20: 32 bounded messages, no timed path (D12, task 9.4) |

### `crates/lushtext-core/src/services/recovery_metadata.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/recovery_metadata.rs` | `DEFAULT_MAX_METADATA_BYTES` | 16,777,216 | presentation cap | up from 16,777,216: past it metadata is refused (D12) |
| `crates/lushtext-core/src/services/recovery_metadata.rs` | `MAX_QUARANTINE_NAME_ATTEMPTS` | 64 | retry budget | power of two |

### `crates/lushtext-core/src/services/responsiveness_log.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `PROBE_LOG_VERSION` | 1 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `EVIDENCE_CHANNEL_CAPACITY` | 4,096 | cap | power of two |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `DEFAULT_LATE_THRESHOLD_US` | 1,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `SECOND_RECORD_INTERVAL_US` | 1,000,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `TOP_K_RECORD_INTERVAL_US` | 5,000,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `TOP_K` | 16 | cap | power of two |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `DEFAULT_FRAME_PHASE_FRAMES` | 4 | cap | power of two |
| `crates/lushtext-core/src/services/responsiveness_log.rs` | `MAX_FRAME_PHASE_FRAMES` | 16,384 | coverage window | rounded up (D5) |

### `crates/lushtext-core/src/services/search_backup.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/search_backup.rs` | `JOURNAL_SCAN_MAX_ENTRIES` | 16,384 | presentation cap | up from 10,000: at least RESULT_CAP (asserted); M12, 33.9-34.5 MiB, 381-392 ms (D12) |

### `crates/lushtext-core/src/services/search_history.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/search_history.rs` | `MAX_HISTORY` | 32 | retention tier | rounded up (D6) |

### `crates/lushtext-core/src/services/workspace_watch.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/services/workspace_watch.rs` | `WORKSPACE_WATCH_PATH_CAP` | 1,024 | cap | power of two |
| `crates/lushtext-core/src/services/workspace_watch.rs` | `WATCH_ERROR_MESSAGE_CAP_BYTES` | 1,024 | presentation cap | up from 1,024: error text a user reads (D12) |

### `crates/lushtext-core/src/ui/accessibility.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/accessibility.rs` | `DEFAULT_ANNOUNCEMENT_LIMIT` | 256 | presentation cap | up from 240: characters announced (D12) |

### `crates/lushtext-core/src/ui/automation.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/automation.rs` | `INTERFACE_VERSION` | 2 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/automation.rs` | `SNAPSHOT_TEXT_MAX_BYTES` | 4,096 | presentation cap | up from 4,096: text an automation snapshot carries (D12) |

### `crates/lushtext-core/src/ui/buffer_snapshot.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/buffer_snapshot.rs` | `BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD` | 8,388,608 | cap | 8 MiB (D3) |
| `crates/lushtext-core/src/ui/buffer_snapshot.rs` | `BUFFER_SNAPSHOT_CHUNK_CHARS` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/buffer_snapshot.rs` | `BUFFER_SNAPSHOT_CHUNK_MAX_BYTES` | 262,144 | cap | power of two |

### `crates/lushtext-core/src/ui/command_palette/item.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/command_palette/item.rs` | `KIND_FILE` | 0 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/command_palette/item.rs` | `KIND_COMMAND` | 1 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/command_palette/item.rs` | `KIND_HEADER` | 2 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/command_palette/item.rs` | `KIND_NOTE` | 3 | out: identifier | format version, discriminant or sentinel (D1) |

### `crates/lushtext-core/src/ui/command_palette/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/command_palette/policy.rs` | `INDEX_UPDATE_DEBOUNCE_MS` | 75 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/command_palette/policy.rs` | `SEARCH_DEBOUNCE_MS` | 150 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/command_palette/policy.rs` | `MAX_RESULTS_PER_SOURCE` | 64 | presentation cap | up from 50: M1, settle worst 6.4-7.9 ms within B_interact (main 5.0-5.7 ms) (D12) |
| `crates/lushtext-core/src/ui/command_palette/policy.rs` | `MAX_PENDING_INDEX_UPDATES` | 1,024 | cap | power of two |
| `crates/lushtext-core/src/ui/command_palette/policy.rs` | `MAX_PENDING_INDEX_UPDATE_BYTES` | 4,194,304 | cap | power of two |
| `crates/lushtext-core/src/ui/command_palette/policy.rs` | `PENDING_QUEUE_MIN_CAPACITY` | 4 | cap | power of two |

### `crates/lushtext-core/src/ui/editor_page/accessibility.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/accessibility.rs` | `EDITOR_IDENTITY_LIMIT` | 128 | presentation cap | up from 96: characters of a file name announced (D12) |

### `crates/lushtext-core/src/ui/editor_page/bookmarks.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/bookmarks.rs` | `BOOKMARK_MARK_PRIORITY` | 150 | out: priority | GLib, CSS, mark or gutter priority (D1) |

### `crates/lushtext-core/src/ui/editor_page/document_scale.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/document_scale.rs` | `REDUCED_FEATURES_NOTICE_LIMIT` | 512 | presentation cap | up from 320: AT text; the longest shipped notice is 229 characters (D12, task 9.4) |

### `crates/lushtext-core/src/ui/editor_page/held_input.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/held_input.rs` | `SETTLE_RETRY_INTERVAL_MS` | 16 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/editor_page/held_input.rs` | `PREEDIT_DEADLINE_MS` | 1,024 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/editor_page/load/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/load/policy.rs` | `CLEAR_SLICE_CHARS` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/load/policy.rs` | `MAX_UTF8_BYTES_PER_CHAR` | 4 | external fact | exact, cited from its source |
| `crates/lushtext-core/src/ui/editor_page/load/policy.rs` | `SETTLE_QUIET_US` | 100,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/editor_page/load/policy.rs` | `INPUT_QUIET_US` | 16,666 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/editor_page/load/policy.rs` | `ATTACH_DEADLINE_US` | 2,048,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/editor_page/local_history.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/local_history.rs` | `DEFAULT_PERIODIC_CAPTURE_INTERVAL_MS` | 300,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_MARKER_STRIP_WIDTH` | 8 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_TOP_CONTENT_MARGIN` | 5 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_WIDE_EDITOR_RATIO_DENOMINATOR` | 5 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_LONG_LINE_WARNING_THRESHOLD` | 120 | external fact | the 120-column style convention; bounds no work, kept exact (stage 4) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_SEARCH_MATCH_CAP` | 2,048 | presentation cap | up from 2,000: M7, every refresh within main + one reference frame (D12) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET` | 2,097,152 | presentation cap | up from 2,097,152: past it markers follow the unwrapped projection (D12) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET` | 8,192 | presentation cap | up from 8,000: M7 (D12) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_ANALYSIS_CHARS_PER_SLICE` | 32,768 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_LONG_LINE_MARK_CAP` | 2,048 | presentation cap | up from 2,000: M7 (D12) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_VIEWPORT_HORIZONTAL_OUTSET` | 13 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` | `MINIMAP_MODIFIED_LINE_MARK_CAP` | 2,048 | presentation cap | up from 2,000: M7 (D12) |

### `crates/lushtext-core/src/ui/editor_page/overscroll.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/overscroll.rs` | `MIN_EDITOR_BOTTOM_MARGIN` | 6 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/editor_page/protected_view/fold_gutter.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/protected_view/fold_gutter.rs` | `FOLD_GUTTER_POSITION` | 8 | out: priority | GLib, CSS, mark or gutter priority (D1) |
| `crates/lushtext-core/src/ui/editor_page/protected_view/fold_gutter.rs` | `FOLD_GUTTER_XPAD` | 4 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `FIND_RESULTS_PER_TURN` | 256 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `FIND_BYTES_PER_TURN` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `FIND_RESULT_CAP` | 16,384 | presentation cap | up from 10,000: M4, delivery worst 7.0-7.6 ms (main 7.2-8.1); records <= 8 MiB (D12) |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `MAX_FIND_RECORD_BYTES` | 512 | presentation cap | up from 512: characters of a find result's preview (task 9.1) (D12) |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `COPY_CHUNK_BYTES` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `SOURCE_CAPTURE_CHUNK_CHARS` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `SOURCE_CAPTURE_STEP_CHARS` | 128 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `NOTICE_QUIET_US` | 4,000,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` | `NOTICE_REASONS` | 7 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |

### `crates/lushtext-core/src/ui/editor_page/protected_view/seams.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/protected_view/seams.rs` | `PARAGRAPH_CHECKPOINT_STRIDE` | 1,024 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/protected_view/seams.rs` | `CHAR_CHECKPOINT_STRIDE` | 65,536 | cap | power of two |

### `crates/lushtext-core/src/ui/editor_page/restyle.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/restyle.rs` | `PACED_REATTACH_MAX_LINES` | 16,384 | cap | power of two |

### `crates/lushtext-core/src/ui/editor_page/save/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/save/policy.rs` | `SAVE_PAYLOAD_SHARED_BUDGET_BYTES` | 268,435,456 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/save/policy.rs` | `SAVE_PAYLOAD_FIXED_OVERHEAD_BYTES` | 1,048,576 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/save/policy.rs` | `SAVE_PAYLOAD_RESIDENCY_MULTIPLIER` | 8 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/ui/editor_page/save/policy.rs` | `MAX_ADMITTED_SAVE_PAYLOADS` | 8 | cap | power of two |

### `crates/lushtext-core/src/ui/editor_page/search.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/search.rs` | `MAX_SEARCH_SELECTION_PREFILL_CHARS` | 1,024 | presentation cap | up from 1,024: selection text prefilled into find (D12) |

### `crates/lushtext-core/src/ui/editor_page/view_attachment.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/editor_page/view_attachment.rs` | `VIEW_DETACH_MAX_LINES` | 262,144 | cap | power of two |
| `crates/lushtext-core/src/ui/editor_page/view_attachment.rs` | `AT_ATTACH_MAX_CHARS` | 1,048,576 | presentation cap | up from 1,048,576: a larger detached document reaches an AT only when shown (D12) |
| `crates/lushtext-core/src/ui/editor_page/view_attachment.rs` | `MAX_DISPLAY_LINE_STEPS` | 8,192 | coverage window | power of two, rounded up |

### `crates/lushtext-core/src/ui/entry_paste_cap.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/entry_paste_cap.rs` | `ENTRY_INSERT_CAP_BYTES` | 16,384 | presentation cap | up from 16,384: pasted text an entry keeps (D12) |

### `crates/lushtext-core/src/ui/font_zoom.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/font_zoom.rs` | `PRUNE_FLOOR` | 64 | cap | power of two |

### `crates/lushtext-core/src/ui/font_zoom/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/font_zoom/policy.rs` | `ZOOM_MIN` | 50 | out: percent | zoom grid (D1) |
| `crates/lushtext-core/src/ui/font_zoom/policy.rs` | `ZOOM_MAX` | 400 | out: percent | zoom grid (D1) |
| `crates/lushtext-core/src/ui/font_zoom/policy.rs` | `ZOOM_STEP` | 10 | out: percent | zoom grid (D1) |
| `crates/lushtext-core/src/ui/font_zoom/policy.rs` | `ZOOM_DEFAULT` | 100 | out: percent | zoom grid (D1) |

### `crates/lushtext-core/src/ui/input_quiet.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/input_quiet.rs` | `INPUT_FRAME_WAIT_US` | 128,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/language_catalog.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/language_catalog.rs` | `CATALOG_BUILD_PRIORITY` | 198 | out: priority | GLib, CSS, mark or gutter priority (D1) |

### `crates/lushtext-core/src/ui/markdown_preview/imp.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `LIST_ITEM_MARKER_MARGIN` | 24 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `LIST_ITEM_MARKER_SLOT` | 36 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `LIST_ITEM_DEPTH_STEP` | 28 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `BLOCKQUOTE_BASE_MARGIN` | 18 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `BLOCKQUOTE_DEPTH_STEP` | 20 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `ALERT_BODY_LEFT_MARGIN` | 24 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `ALERT_BODY_RIGHT_MARGIN` | 16 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `FOOTNOTE_DEF_LEFT_MARGIN` | 32 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `FOOTNOTE_DEF_RIGHT_MARGIN` | 16 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `DEFINITION_DEF_LEFT_MARGIN` | 32 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | `DEFINITION_DEF_RIGHT_MARGIN` | 16 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/markdown_preview/mod.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_PREVIEW_IMAGE_WIDTH` | 640 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MIN_PREVIEW_IMAGE_SIZE` | 72 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `CODE_BLOCK_HORIZONTAL_PADDING` | 12 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `CODE_BLOCK_VERTICAL_PADDING` | 8 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `CODE_BLOCK_BACKGROUND_CSS_PRIORITY` | `expr: gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 2` | out: priority | GLib, CSS, mark or gutter priority (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_PREVIEW_IMAGE_SOURCE_BYTES` | 33,554,432 | presentation cap | up from 33,554,432: a larger image is not shown (D12) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_PREVIEW_IMAGE_SOURCE_PIXELS` | 67,108,864 | presentation cap | up from 64,000,000: M16, decode peak 297.4-298.0 MiB vs main 285.2-286.1 (D12) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_PREVIEW_IMAGE_WORK_ITEMS` | 4 | cap | power of two |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `PREVIEW_IMAGE_WORK_CHARGE_BYTES` | `expr: MAX_PREVIEW_IMAGE_SOURCE_BYTES.saturating_add( (MAX_PREVIEW_IMAGE_WIDTH as u64) .saturating_pow(2) .saturating_mul(4), )` | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_PREVIEW_IMAGE_WORK_BYTES` | `expr: PREVIEW_IMAGE_WORK_CHARGE_BYTES.saturating_mul(MAX_PREVIEW_IMAGE_WORK_ITEMS as u64)` | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_PREVIEW_CODE_BLOCK_BYTES` | 65,536 | presentation cap | up from 65,536: a larger code block falls back (D12) |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MARKDOWN_RETIREMENT_CHARS_PER_TURN` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MARKDOWN_RETIREMENT_ITEMS_PER_TURN` | 64 | cap | power of two |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | `MAX_MARKDOWN_RETIREMENT_GENERATIONS` | 2 | cap | power of two |

### `crates/lushtext-core/src/ui/markdown_preview/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/markdown_preview/policy.rs` | `MAX_INLINE_FOOTNOTE_REPLACEMENTS` | 16,384 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/ui/markdown_preview/policy.rs` | `LOWERING_CANCEL_CHECK_BYTES` | 4,096 | cap | power of two |
| `crates/lushtext-core/src/ui/markdown_preview/policy.rs` | `PREVIEW_MIN_WIDTH_SP` | 1 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/policy.rs` | `PREVIEW_DEFAULT_WIDTH_SP` | 300 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/markdown_preview/policy.rs` | `PREVIEW_MAX_WIDTH_DIVISOR` | 3 | out: factor | factor, divisor or unit (D1) |

### `crates/lushtext-core/src/ui/markdown_preview/seams.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/markdown_preview/seams.rs` | `MARKDOWN_PLAN_RESERVATION_BYTES` | 12,582,912 | derived | computed from named inputs; not rounded (D2) |

### `crates/lushtext-core/src/ui/markdown_preview/tables.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/markdown_preview/tables.rs` | `MAX_PREVIEW_TABLE_CELLS` | 1,024 | presentation cap | up from 1,000: M13 (D12) |

### `crates/lushtext-core/src/ui/open_popover/imp.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/open_popover/imp.rs` | `RECENT_SCROLLER_MAX_HEIGHT` | 600 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/open_popover/imp.rs` | `RECENT_SCROLLER_CONTENT_WIDTH` | 250 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/plain_disposal.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `DISPOSAL_WORKERS` | 2 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `DISPOSAL_QUEUE_CAPACITY` | 8 | cap | power of two |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `DISPOSAL_RETAINED_BYTE_CAPACITY` | 134,217,728 | cap | power of two |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `FILE_LOAD_OVERWEIGHT_PROGRESS_CAPACITY` | `expr: crate::model::document_scale::REFUSE_TO_OPEN .saturating_mul(crate::model::file_load::DECODED_BODY_SOURCE_MULTIPLIER)` | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY` | 75,497,472 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `MAX_SMALL_PENDING_DISPOSAL_BYTES #[cfg(feature="test-utils")]` | 65,536 | cap | power of two |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `aggregate_pressure_evidence_for_test::MIB` | 1,048,576 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `aggregate_pressure_evidence_for_test::PRODUCER_COUNT` | 4 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |
| `crates/lushtext-core/src/ui/plain_disposal.rs` | `aggregate_pressure_evidence_for_test::SUBMISSIONS_PER_PRODUCER` | 3 | out: tool | proof tool, fixture, fuzz or test-helper data (D1) |

### `crates/lushtext-core/src/ui/preferences/data_page.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/preferences/data_page.rs` | `DATA_DETAILS_MAX_ROWS_PER_GROUP` | 32 | presentation cap | up from 32: rows listed before the omitted-count row (D12) |

### `crates/lushtext-core/src/ui/preferences/preserved_drafts.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/preferences/preserved_drafts.rs` | `SET_ASIDE_LIST_MAX_HEIGHT` | 320 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/responsiveness_probe.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/responsiveness_probe.rs` | `measurement::DEFAULT_TICK_US` | 2,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/responsiveness_probe.rs` | `measurement::MAX_PENDING_FRAMES` | 64 | cap | power of two |
| `crates/lushtext-core/src/ui/responsiveness_probe.rs` | `measurement::RECENT_LATE_FLOOR_US` | 1,000 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/responsiveness_probe.rs` | `measurement::RECENT_LATE_CAPACITY` | 1,024 | cap | power of two |

### `crates/lushtext-core/src/ui/search_panel/execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/search_panel/execution.rs` | `MAX_SEARCH_EVENTS_PER_TICK` | 256 | cap | 256: a full turn stays within B_settle (D8, task 5.4) |

### `crates/lushtext-core/src/ui/search_panel/imp.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | `SEARCH_INPUT_DEBOUNCE_MS` | 300 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/search_panel/item.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/search_panel/item.rs` | `imp::KIND_FILE` | 0 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/search_panel/item.rs` | `imp::KIND_MATCH` | 1 | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/search_panel/item.rs` | `imp::SearchResultItem::KIND_FILE` | `expr: KIND_FILE` | out: identifier | format version, discriminant or sentinel (D1) |
| `crates/lushtext-core/src/ui/search_panel/item.rs` | `imp::SearchResultItem::KIND_MATCH` | `expr: KIND_MATCH` | out: identifier | format version, discriminant or sentinel (D1) |

### `crates/lushtext-core/src/ui/search_panel/journal.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/search_panel/journal.rs` | `report_startup_diagnostics::DETAIL_LIMIT` | 8 | out: presentation | presentation shape: rows or lines a surface shows (D1) |

### `crates/lushtext-core/src/ui/search_panel/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/search_panel/policy.rs` | `PREVIEW_ROW_RETAINED_BYTES` | `expr: std::mem::size_of::<Replacement>() .saturating_add(std::mem::size_of::<Option<usize>>()) .saturating_add(std::mem::size_of::<SearchMatchId>())` | derived | computed from named inputs; not rounded (D2) |

### `crates/lushtext-core/src/ui/search_panel/retirement.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/search_panel/retirement.rs` | `SEARCH_RETIREMENT_ROWS_PER_SLICE` | 256 | cap | 256: a full turn stays within B_settle (D8, task 5.4) |
| `crates/lushtext-core/src/ui/search_panel/retirement.rs` | `MAX_SEARCH_RETIREMENT_GENERATIONS` | 2 | cap | power of two |

### `crates/lushtext-core/src/ui/settings_writes.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/settings_writes.rs` | `SCHEMA_KEYS_HINT` | 64 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |

### `crates/lushtext-core/src/ui/sidebar/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/policy.rs` | `MAX_UNIQUE_NAME_ATTEMPTS` | 1,024 | retry budget | rounded up (D6) |
| `crates/lushtext-core/src/ui/sidebar/policy.rs` | `PERSIST_DEBOUNCE_MS` | 150 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/sidebar/policy.rs` | `REVEAL_SETTLE_FRAME_BUDGET` | 64 | coverage window | rounded up (D5) |

### `crates/lushtext-core/src/ui/sidebar/reveal_execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/reveal_execution.rs` | `REVEAL_NEIGHBOURS_PER_SIDE` | 4 | out: presentation | presentation shape: rows or lines a surface shows (D1) |

### `crates/lushtext-core/src/ui/sidebar/workspace_section/peek_execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/workspace_section/peek_execution.rs` | `PEEK_CARD_WIDTH` | 520 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/peek_execution.rs` | `PEEK_CARD_HEIGHT` | 420 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/peek_execution.rs` | `PEEK_CARD_X_OFFSET` | 15 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/sidebar/workspace_section/refresh_execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/workspace_section/refresh_execution.rs` | `AUTO_REFRESH_DEBOUNCE_MS` | 120 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/refresh_execution.rs` | `MANUAL_REFRESH_DEBOUNCE_MS` | 1 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/refresh_execution.rs` | `REFRESH_SCAN_DISPATCH_BATCH` | 32 | cap | power of two |

### `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_admission.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_admission.rs` | `WORKSPACE_SCAN_TASK_LIMIT` | 4 | cap | power of two |

### `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_execution.rs` | `MAX_DIR_ENTRIES` | 10,000 | presentation cap, exception | up from 10,000, kept: M5 measured the terminal dispatch of a capped expansion at 25-30 ms (main, 10,000) and 41-43 ms (16,384), past main + one reference frame (D12) |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_execution.rs` | `CHILD_APPEND_BATCH_SIZE` | 256 | cap | power of two |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_execution.rs` | `CHILD_RECONCILE_DIRECT_ROWS` | 256 | cap | power of two |

### `crates/lushtext-core/src/ui/sidebar/workspace_section/watch.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/sidebar/workspace_section/watch.rs` | `WATCH_POLL_MS` | 100 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/watch.rs` | `WATCH_RESTART_SETTLE_MS` | 25 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/window/composition/execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/composition/execution.rs` | `FIRST_FRAME_ICON_SIZE` | 16 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/window/drafts/mod.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/drafts/mod.rs` | `DRAFT_RESTORE_DISPOSAL_RESERVATION_BYTES` | `expr: draft_service::MAX_AUTOMATIC_DRAFT_BYTES.saturating_add(1024 * 1024)` | derived | computed from named inputs; not rounded (D2) |

### `crates/lushtext-core/src/ui/window/drafts/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/drafts/policy.rs` | `FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS` | 750 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/window/editor_focus/mod.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/editor_focus/mod.rs` | `EDITOR_FOCUS_MAX_ATTEMPTS` | 8 | retry budget | rounded up (D6) |

### `crates/lushtext-core/src/ui/window/focus_mode/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/focus_mode/policy.rs` | `MIN_FOCUS_MARGIN` | 24 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/focus_mode/policy.rs` | `PANGO_UNITS_PER_PIXEL` | 1,024 | external fact | exact, cited from its source |

### `crates/lushtext-core/src/ui/window/geometry/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `WORKSPACE_SIDEBAR_MIN_WIDTH_SP` | 1 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `PROPERTIES_SIDEBAR_MIN_WIDTH_SP` | 280 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `NORMAL_MODE_MIN_HEIGHT_SP` | 360 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `WORKSPACE_BREAKPOINT_MAX_WIDTH_SP` | 860 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP` | 400 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `FIXED_PROPERTIES_SIDEBAR_DIVISOR` | 4 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `MIN_EDITOR_CONTENT_WIDTH_SP` | 620 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/geometry/policy.rs` | `DUAL_PANE_LAYOUT_OVERHEAD_SP` | 32 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/window/imp.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/imp.rs` | `PREVIEW_SETTLE_DELAY_MS` | 16 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/window/imp.rs` | `WORKSPACE_SIDEBAR_TRANSITION_SETTLE_DELAY_MS` | 260 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/window/imp.rs` | `LushtextWindow::record_layout_probe_span::LAYOUT_PROBE_SPANS` | 16 | out: structural | enum length, bucket or worker count, or capacity hint (D1) |

### `crates/lushtext-core/src/ui/window/local_history/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_PARENT_MARGIN_SP` | 48 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_WIDTH_PERMILLE` | 900 | out: percent | percentage, permille or zoom percent (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_HEIGHT_PERMILLE` | 880 | out: percent | percentage, permille or zoom percent (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_MIN_WIDTH_SP` | 1,080 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_MAX_WIDTH_SP` | 1,680 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_MIN_HEIGHT_SP` | 720 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `VIEWER_MAX_HEIGHT_SP` | 1,080 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `EMPTY_WIDTH_SP` | 640 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `EMPTY_HEIGHT_SP` | 480 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `PREVIEW_DIRECT_THRESHOLD_BYTES` | 524,288 | derived | computed from named inputs; not rounded (D2) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `PREVIEW_RESERVATION_BYTES` | 67,108,864 | presentation cap | up from 67,108,864: past it a history preview is refused (D12) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `format_bytes::KIB` | 1,024 | out: factor | factor, divisor or unit (D1) |
| `crates/lushtext-core/src/ui/window/local_history/policy.rs` | `format_bytes::MIB` | 1,048,576 | out: factor | factor, divisor or unit (D1) |

### `crates/lushtext-core/src/ui/window/local_history/preview_execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/local_history/preview_execution.rs` | `OWNED_SNAPSHOT_RETRY_MS` | 4 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/window/local_history/preview_execution.rs` | `PREVIEW_DIRECT_CLEAR_CHARS` | 65,536 | cap | power of two |

### `crates/lushtext-core/src/ui/window/notes/browser.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/notes/browser.rs` | `NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP` | 12 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/notes/browser.rs` | `NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP` | 10 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/notes/browser.rs` | `NOTES_BROWSER_WIDTH_SP` | 980 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/notes/browser.rs` | `NOTES_BROWSER_HEIGHT_SP` | 700 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/window/notes/editor_execution.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/notes/editor_execution.rs` | `NOTE_EDITOR_SURFACE_WIDTH_SP` | 520 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/notes/editor_execution.rs` | `NOTE_EDITOR_SURFACE_HEIGHT_SP` | 300 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/notes/editor_execution.rs` | `NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP` | 12 | out: geometry | UI geometry in sp or px (D1) |
| `crates/lushtext-core/src/ui/window/notes/editor_execution.rs` | `NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP` | 10 | out: geometry | UI geometry in sp or px (D1) |

### `crates/lushtext-core/src/ui/window/notes/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_RENDER_LIMIT` | 512 | presentation cap | up from 500: M6, worst dispatch 34-40 ms vs main 32-38 ms (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_SOURCE_ENTRY_LIMIT` | 16,384 | presentation cap | up from 10,000: M12 (palette note source, same admission engine) (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_SOURCE_TEXT_LIMIT` | 67,108,864 | presentation cap | up from 67,108,864: past it the browser source is truncated (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_SIDECAR_SCAN_LIMIT` | 16,384 | presentation cap | up from 10,000: M12; the 8 MiB sidecar-path ledger bounds it (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_DIAGNOSTIC_LIMIT` | 1,024 | presentation cap | up from 1,024: diagnostics the user is shown (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT` | 16,384 | presentation cap | up from 10,000: M6, capture in the open action 9.4-13.4 ms within B_settle (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT` | 4,194,304 | presentation cap | up from 4,194,304: past it open-editor notes are dropped (D12) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_SAVE_DEBOUNCE_MS` | 200 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `MAX_BOOKMARK_SAVE_ATTEMPTS` | 4 | retry budget | rounded up (D6) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS` | 80 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS` | 150 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |
| `crates/lushtext-core/src/ui/window/notes/policy.rs` | `NOTES_BROWSER_SEARCH_DEBOUNCE_MS` | 150 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/window/preview.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/preview.rs` | `PREVIEW_SIDEBAR_ALLOCATION_MAX_FRAMES` | 8 | coverage window | power of two, rounded up |

### `crates/lushtext-core/src/ui/window/recent_documents_journal.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/recent_documents_journal.rs` | `RECENT_DOCUMENTS_SAVE_DEBOUNCE_MS` | 250 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/window/search.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/search.rs` | `SEARCH_PANEL_TRANSITION_DELAY_MS` | 260 | out: time | time unit; time budgets follow the rule already (D1 non-goal) |

### `crates/lushtext-core/src/ui/window/session_restore/policy.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/session_restore/policy.rs` | `SESSION_RESTORE_PAGES_PER_TURN` | 4 | cap | power of two |
| `crates/lushtext-core/src/ui/window/session_restore/policy.rs` | `SESSION_RESTORE_FILE_PLAN_PERMITS` | 2 | cap | power of two |

### `crates/lushtext-core/src/ui/window/startup_data.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/startup_data.rs` | `MAX_PENDING_ACTIVATION_OPENS` | 64 | presentation cap | up from 64: files a CLI open admits (D12) |

### `crates/lushtext-core/src/ui/window/zoom.rs`

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/lushtext-core/src/ui/window/zoom.rs` | `ZOOM_MIN` | 50 | out: percent | zoom grid (D1) |
| `crates/lushtext-core/src/ui/window/zoom.rs` | `ZOOM_MAX` | 400 | out: percent | zoom grid (D1) |
| `crates/lushtext-core/src/ui/window/zoom.rs` | `ZOOM_STEP` | 10 | out: percent | zoom grid (D1) |
| `crates/lushtext-core/src/ui/window/zoom.rs` | `ZOOM_DEFAULT` | 100 | out: percent | zoom grid (D1) |
