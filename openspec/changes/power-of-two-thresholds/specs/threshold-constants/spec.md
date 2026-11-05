## ADDED Requirements

### Requirement: Every threshold is a power of two or a recorded exception
Every integer constant in the production crates' sources is either a
threshold or out of scope.

A constant is a threshold when it bounds work per dispatch, memory, retained
entries or retries, or decides a feature or tier from a size, length or
count. Each threshold SHALL have one of these kinds:

- cap (an internal resource cap);
- presentation cap;
- coverage window;
- policy tier;
- retention tier;
- retry budget;
- external fact;
- derived;
- measurement record;
- recorded exception.

The kind sets the value:

- A **cap** SHALL be the measured or derived value rounded down to a power of
  two. A cap SHALL bound only internal resources: nothing a user can observe
  may depend on its value.
- A **presentation cap** is a cap on what the user sees or gets: results,
  rows, notes, markers, entries listed, files a bulk action processes,
  characters shown or announced, and the size or length at which a feature
  turns off. A cap SHALL be classified as a presentation cap when rounding it
  down could hide, skip, truncate or drop anything a user could observe, and
  when in doubt. A presentation cap SHALL be the power of two at or above its
  value before `power-of-two-thresholds` (its intended value, for a newer
  constant) when the resource bound it protects (memory, per-turn GTK time
  against `B_INTERACT_US` or `B_SETTLE_US`, worker memory) holds at that
  power of two by measurement; otherwise it SHALL keep that former value
  exactly, as a recorded exception with the measurement. A presentation cap
  SHALL NOT be lower than its value on `main` before this change.
- A **coverage window**, **retention tier** or **retry budget** SHALL be
  rounded up to a power of two.
- A **policy tier** SHALL be a power of two aligned with its neighbours for
  the same concern.
- An **external fact** or **measurement record** SHALL stay exact and cite
  its source.
- A **derived** constant SHALL be computed from its named inputs and SHALL
  NOT be rounded.
- A **recorded exception** SHALL keep an exact value only when the power of
  two at or below it costs a measured user-visible loss or breaks its bound,
  and the power of two above it is measured to break its bound or to cost a
  loss. Its documentation SHALL state those measurements. A presentation cap
  that is an exception SHALL keep its former value.
- A **policy tier** at which a feature turns off SHALL also follow the
  presentation-cap rule.

Accessible-text and announcement lengths are presentation caps, and none
SHALL be below the longest string the app ships. A derived value the user
sees SHALL NOT fall below its value on `main` because one of its inputs
changed, unless a measurement shows that keeping it breaks its bound and the
reason is recorded.

Corpus counts of the files that would lose a feature at the power of two
below a presentation cap SHALL stay recorded beside it as evidence; they do
not permit rounding it down.

A retention tier or retry budget raised by this rule SHALL keep its store
within every bounded read that loads it: a store holding the raised number
of entries at their largest admissible size SHALL load in full, or the read
bound SHALL be raised with it as a coverage bound.

Each threshold's rustdoc SHALL contain a `Kind:` line naming its kind and,
for a non-power-of-two value, the reason.

Retention tiers SHALL NOT be lowered by this change. No stored local history,
recent document, search history or preserved draft SHALL be trimmed because
of it.

#### Scenario: A cap is rounded down
- **WHEN** an internal cap's measured value lies between two powers of two
- **THEN** the constant takes the lower one, unless a measurement at the higher one meets the same budget criterion, in which case it may take the higher one and records that measurement

#### Scenario: A presentation cap is rounded up when its bound holds
- **WHEN** a palette query matches 60 files in one category, which `main` (50 per category) showed 50 of
- **THEN** the category shows all 60, because `MAX_RESULTS_PER_SOURCE` is 64 and the measured splice of every full category stays within `B_INTERACT_US`

#### Scenario: A presentation cap whose bound breaks above keeps its former value
- **WHEN** the measurement that derived a presentation cap shows the power of two above it breaking its bound, as C2's head sweep does for `N_HEAD` (16.1 ms at 14,000 characters)
- **THEN** the cap keeps its value on `main` (`N_HEAD` 10,000) as a recorded exception citing that measurement, and is not rounded down

#### Scenario: No presentation cap shows less than main
- **WHEN** a workspace search, a Replace Preview, a Protected View find or a sidebar directory reaches 10,000 matches, rows or entries
- **THEN** every one of them is delivered, as on `main`, because the first three caps are 16,384 and the sidebar directory cap keeps `main`'s 10,000 as a recorded exception

#### Scenario: A coverage window is rounded up
- **WHEN** `R_BATCH` must cover the 223 rows a 2000 px validation batch can touch at the 50 % zoom
- **THEN** it is 256, `R_FOLD` equals it, and `N_DENSE` is derived as `V_BATCH / R_BATCH`

#### Scenario: A visible loss keeps an exception
- **WHEN** rounding `L_SYNTAX` down would remove highlighting from a measured share of ordinary Markdown files and rounding it up would break `MAX_SYNTAX_PARAGRAPH_BYTES ≤ HIGHLIGHT_CHUNK_BYTES`
- **THEN** `L_SYNTAX` stays 2,000 and its rustdoc names the kind "recorded exception" with both measurements

#### Scenario: A feature gate is never rounded down
- **WHEN** rounding the CSS attach gate down to 32 KiB would remove highlighting from no corpus file, and A69's sweep derived 37 KiB as the largest CSS size whose attach stays within `B_SETTLE_US`
- **THEN** the gate stays 37 KiB as a presentation-cap exception citing A69, because a presentation cap is never rounded down and the power of two above breaks its bound

#### Scenario: Replace All processes every file main processed
- **WHEN** a Replace All run touches files of up to 16 MiB each whose total undo payload is within 64 MiB
- **THEN** every file is replaced, including those between 10 MiB and 16 MiB that `main` skipped, and the aggregate undo payload still bounds the run's memory
- **AND** when the undo payload runs out, every file `main` would have replaced is replaced before any file above 10 MiB is charged, so no larger file displaces one

#### Scenario: A raised retention tier still loads in full
- **WHEN** the recent-documents file holds 256 entries whose path and canonical path are each 4,096 bytes long
- **THEN** the file is within `MAX_RECENT_DOCUMENTS_BYTES` and every entry loads

#### Scenario: Upgrading keeps stored history
- **WHEN** a user upgrades with 48 local-history snapshots for one file, 240 in total, and 20 recent searches
- **THEN** every stored snapshot and search remains after the first save and the first new search

### Requirement: A committed inventory gates every integer constant
The repository SHALL commit a threshold inventory, `docs/thresholds.md`. It
SHALL have one row for every integer `const` and `static` in the production
crates' `src/` trees, excluding:

- `#[cfg(test)]` modules;
- `kani_proofs.rs` files;
- the `gtk-lush-axioms` probe parameters.

Each row SHALL give the constant's path, module-qualified name, value, kind
or out-of-scope class, and its rounding or reason. The check SHALL read
declarations that span several lines.

`make check-thresholds`, run by `make check-policy`, SHALL fail on any of
these:

- a constant with no row;
- a row with no constant;
- a value that differs from its row;
- an in-scope constant that is not a power of two and whose kind is not
  derived, external fact, measurement record or recorded exception;
- a presentation cap whose value is not the power of two at or above the
  former value its row records, or, for a recorded exception, not that
  former value;
- an in-scope constant whose rustdoc `Kind:` line is missing or disagrees
  with its row.

The check SHALL have a self-test that fails first on a fixture tree.

#### Scenario: A new threshold must be classified
- **WHEN** a change adds `const MAX_THINGS: usize = 1000;` to a production module without an inventory row
- **THEN** `make check-thresholds` fails, naming the constant

#### Scenario: A non-power-of-two cap is refused
- **WHEN** an inventory row classifies a constant of value 10,000 as a cap
- **THEN** the check fails until the value is 8,192 or the row is a recorded exception with its reason

#### Scenario: A presentation cap rounded down is refused
- **WHEN** an inventory row classifies a constant of value 8,192 as a presentation cap whose recorded former value is 10,000
- **THEN** the check fails until the value is 16,384 or the row is a recorded exception at 10,000

#### Scenario: Same-named constants in different modules are distinct rows
- **WHEN** two inline modules of one file each declare `const JSON: u64` with different values
- **THEN** the inventory has one row per module-qualified name, and a value change to either fails the check against its own row

#### Scenario: An out-of-scope constant needs only its class
- **WHEN** a constant is UI geometry, a percentage, a GLib priority or a fixture size
- **THEN** its row names that class and the check accepts any value

### Requirement: User-visible size tiers are shown in IEC units
Every message, notice and accessible text that names a size tier SHALL print
it in IEC units (`KiB`, `MiB`), as `glib::format_size` with IEC units does.
This covers:

- the open refusal and the grew-while-opening refusal;
- the local-history browse limit and its unavailable message;
- the Replace All per-file and undo-payload skip messages;
- the reduced-features notice;
- the editor's accessible description.

The README SHALL state the same tiers in the same units. Every surface that
displays a file or snapshot size SHALL print it in the same IEC units through
one formatter: the properties panel, the file peek and the local-history
list.

#### Scenario: Local history names its limit in MiB
- **WHEN** the user opens Local History for a 70 MiB file
- **THEN** the message says local history is unavailable for files above 64 MiB

#### Scenario: Displayed file sizes use IEC units
- **WHEN** the properties panel, the file peek or a local-history row shows the size of a 10,485,760-byte file
- **THEN** each shows the same IEC text, "10 MiB", and none prints "MB" or "KB"

#### Scenario: The open refusal names the tier in MiB
- **WHEN** the user opens a file above the open tier
- **THEN** the error names the tier in MiB, together with the file's size in MiB
