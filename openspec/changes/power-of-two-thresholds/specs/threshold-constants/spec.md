## ADDED Requirements

### Requirement: Every threshold is a power of two or a recorded exception
Every integer constant in the production crates' sources is either a
threshold or out of scope.

A constant is a threshold when it bounds work per dispatch, memory, retained
entries or retries, or decides a feature or tier from a size, length or
count. Each threshold SHALL have one of these kinds:

- cap;
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
  two.
- A **coverage window**, **retention tier** or **retry budget** SHALL be
  rounded up to a power of two.
- A **policy tier** SHALL be a power of two aligned with its neighbours for
  the same concern.
- An **external fact** or **measurement record** SHALL stay exact and cite
  its source.
- A **derived** constant SHALL be computed from its named inputs and SHALL
  NOT be rounded.
- A **recorded exception** SHALL keep an exact value only when the power of
  two at or below it, and any measured power of two above it, cost a measured
  user-visible loss. Its documentation SHALL state that measurement.

A rounding that turns a feature off for files that have it today is a
**measured user-visible loss** when, in a corpus of at least 2,000 files that
have the feature at the current value, at least 0.05 % (one in 2,000) of them
lose it at the rounded value. A smaller measured share, or a corpus count of
zero, is no loss: the constant is rounded and the count is recorded beside it.
A sample of fewer than 2,000 such files decides nothing and SHALL be
recounted on a larger corpus.

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
- **WHEN** a cap's measured value lies between two powers of two
- **THEN** the constant takes the lower one, unless a measurement at the higher one meets the same budget criterion, in which case it may take the higher one and records that measurement

#### Scenario: A coverage window is rounded up
- **WHEN** `R_BATCH` must cover the 223 rows a 2000 px validation batch can touch at the 50 % zoom
- **THEN** it is 256, `R_FOLD` equals it, and `N_DENSE` is derived as `V_BATCH / R_BATCH`

#### Scenario: A visible loss keeps an exception
- **WHEN** rounding `L_SYNTAX` down would remove highlighting from a measured share of ordinary Markdown files and rounding it up would break `MAX_SYNTAX_PARAGRAPH_BYTES ≤ HIGHLIGHT_CHUNK_BYTES`
- **THEN** `L_SYNTAX` stays 2,000 and its rustdoc names the kind "recorded exception" with both measurements

#### Scenario: A loss below the materiality floor does not keep an exception
- **WHEN** rounding a per-language syntax gate down removes highlighting from none of the 2,353 corpus CSS files that have it today
- **THEN** the gate takes the power of two below it, and its rustdoc records the corpus count

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
- an in-scope constant whose rustdoc `Kind:` line is missing or disagrees
  with its row.

The check SHALL have a self-test that fails first on a fixture tree.

#### Scenario: A new threshold must be classified
- **WHEN** a change adds `const MAX_THINGS: usize = 1000;` to a production module without an inventory row
- **THEN** `make check-thresholds` fails, naming the constant

#### Scenario: A non-power-of-two cap is refused
- **WHEN** an inventory row classifies a constant of value 10,000 as a cap
- **THEN** the check fails until the value is 8,192 or the row is a recorded exception with its reason

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

The README SHALL state the same tiers in the same units.

#### Scenario: Local history names its limit in MiB
- **WHEN** the user opens Local History for a 70 MiB file
- **THEN** the message says local history is unavailable for files above 64 MiB

#### Scenario: The open refusal names the tier in MiB
- **WHEN** the user opens a file above the open tier
- **THEN** the error names the tier in MiB, together with the file's size in MiB
