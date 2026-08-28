# `services/file_tree.rs` field-deletion survivors — triage (task 10.7)

Slot 4 handed these on, slot 5a re-handed them, and this change owns the triage
because the `mutation-testing` amendment it lands requires the owning change to
triage rather than pass them on again.

`services/file_tree.rs` behaviour is **unchanged** by this change. Only its
`#[cfg(test)]` module gains tests.

## 1. The population is 12 generated, not 11 surviving

The inherited handoff said "**11** pre-existing surviving field-deletion mutants".
Those are two different quantities, and conflating them is why the number never
settled:

- **12 are generated** in this file, enumerated from the tool
  (`MUTANTS_RE='ZZZ_NO_SUCH_MUTANT_ZZZ' make mutants-list` → the 34-mutant
  unfilterable floor, of which 12 are here);
- **11 were recorded surviving**, so exactly **one was already killed** — the
  pre-existing `test_bounded_scan_reports_read_errors` kills the `error` field in
  `scan_directory_without_byte_limit`.

All twelve are `DirectoryScan` struct-literal fields in **early-return paths** —
error and cancellation — of the two scan functions. None is in a success path; the
success-path literals' field deletions are already killed and never appeared in the
survivor list.

| Site | Function | Path | Fields |
| --- | --- | --- | --- |
| A | `scan_directory_bounded_with_cancel_and_bytes` | first-pass read error | `examined_entries`, `error` |
| B | same | cancellation | `examined_entries`, `cancelled` |
| C | same | second-pass read error | `examined_entries`, `peak_retained_entries`, `peak_retained_bytes`, `error` |
| D | `scan_directory_without_byte_limit` | read error | `examined_entries`, `peak_retained_entries`, `peak_retained_bytes`, `error` |

## 2. Step one of the documented order: is each a real missed behaviour?

This is where the triage actually resolves, and it required reading the **backend**
rather than the scan functions. `sys::visit_directory_entries` (the Unix path) has
exactly two error exits and one silent skip:

```rust
let fd = rustix::fs::openat(...).map_err(io::Error::from)?;   // (1) before any entry
let mut dir = rustix::fs::Dir::new(fd).map_err(io::Error::from)?;
while let Some(entry) = dir.read() {
    let entry = entry.map_err(io::Error::from)?;              // (2) mid-iteration
    ...
    let Ok(stat) = rustix::fs::statat(...) else { continue };  // per-entry failure SKIPS
    ...
}
```

Consequences, and they decide the whole triage:

- **The only deterministically reachable error is (1), `openat`** — a missing path, a
  non-directory, or an unreadable directory. It fails **before any entry is visited**,
  so on that path `examined_entries`, `peak_retained_entries`, and
  `peak_retained_bytes` are **already zero**.
- **Deleting a field yields its `Default`**, which for those three is **also zero**.
  So on every reachable error path those mutants are **indistinguishable from the
  original** — they are *equivalent mutants*, not missing tests.
- Error (2), a mid-iteration `getdents` failure, *would* distinguish them, but it is
  not reproducible without fault injection, and `.agents/rules/build.md` forbids
  depending on ambient Unix permissions for failure fixtures precisely because CI may
  run as root.
- A per-entry `statat` failure **`continue`s** rather than erroring, so it cannot
  produce a non-zero-count error path either.
- **Site C is doubly unreachable**: it is the *second* pass over the same directory.
  If the first pass opened it, the second will too, so reaching C requires the
  directory to disappear between passes — a race, not a test.

## 3. What was killable, and is now killed

Three mutants are genuinely distinguishable, and all three are on the **byte-bounded**
function — which the pre-existing tests never exercised at all, because
`scan_directory_bounded` routes to the no-byte-limit variant. That was the real gap:
not a weak assertion, but an **untested function path**.

| Mutant | Why it was distinguishable | Test added |
| --- | --- | --- |
| A `error` | the byte-bounded function's own error literal was never asserted | `byte_bounded_scan_reports_read_errors_on_its_own_path` |
| B `cancelled` | same function, cancellation path never asserted | `byte_bounded_scan_reports_cancellation_with_the_entries_it_had_examined` |
| B `examined_entries` | **the one counter that is observably non-zero**: the cancel check runs *before* the counter increments, so allowing two entries through and refusing the third leaves exactly `2` | same test, `assert_eq!(scan.examined_entries, 2)` |

A third test, `a_pre_cancelled_byte_bounded_scan_examines_nothing`, pins the boundary
case so the mid-walk test's `2` cannot be mistaken for an accident of ordering.

The two error-path tests also assert `examined_entries == 0` explicitly. That does
**not** kill the mutant — it cannot — but it **checks the reachability argument above
rather than leaving it as prose**, so if a future change makes a non-zero count
reachable on an error path, the assertion fails and this triage must be redone.

## 4. Disposition of the remaining eight

**Recorded as equivalent, and deliberately NOT excluded.**

Adding eight `exclude_re` entries would be exactly the widening the mutation policy
forbids — the file already carries one narrowly scoped entry for the `classify_entry`
symlink guard, and that is the shape a legitimate exclusion takes: one operator, one
function, one project-specific rationale. Eight entries covering three whole struct
literals would suppress the success-path fields too if the literals ever moved.

**The durable fix is a fault-injection seam, not an exclusion.**
`.agents/rules/build.md` already blesses the pattern and names the precedent:
*"Prefer feature-gated, per-invocation fault seams with no global mutable state ...
keyed by the exact operation target"* — the Replace/Undo after-metadata hook registry
is the existing instance. A seam that makes `dir.read()` fail after N entries would
make all eight distinguishable in one stroke, and would also let the *success*-path
metrics be asserted under partial failure.

That seam belongs in `services/filesystem/sys.rs`, is a new surface in the filesystem
backend, and is **out of this change's scope** — this change alters no filesystem
behaviour and adds no production code to that boundary. It is handed on with that
concrete shape rather than as an open question.

**This is a partial triage, and it is recorded as partial**: three of twelve killed,
one already killed, eight analysed to a firm "equivalent without a seam" with the
seam named. That is materially better information than the "11 untriaged survivors"
this change received, but it is not a closed item, and the next slot touching this
file inherits it with the analysis rather than the mystery.

## 5. Measured result

Run in a **clean worktree** rather than the main checkout, for a reason worth
recording:

```
git worktree add -f /tmp/w5bnow HEAD
cp <working-tree file_tree.rs> /tmp/w5bnow/...
cd /tmp/w5bnow
MUTANTS_SMOKE_FILE=crates/lushtext-core/src/services/file_tree.rs ./scripts/run-mutants.sh smoke
```

**The wrapper's `smoke` mode passes `--no-config`, which discards
`gitignore = true`.** In the main checkout that makes cargo-mutants attempt to copy a
**97 GB** local `.flatpak-builder/` tree into a 47 GB tmpfs, failing with
`Disk quota exceeded (os error 122)` before testing a single mutant — a confusing
message that says nothing about gitignore. Earlier `diff`-mode runs in this change
were unaffected because they honour the config. Use a clean worktree, or `diff` mode,
for any `smoke`-mode run in a checkout that has local build trees.

### Verified per mutant, by hand-applying each deletion

A full 66-mutant `smoke` run was started for corroboration but is slow (each mutant
rebuilds in a fresh worktree) and was stopped in favour of **direct per-mutant
verification**, which is decisive here: unlike an operator mutation, a field deletion
has no precedence subtlety — the field takes its `Default` and nothing else changes.

Each deletion was applied to the source, the `services::file_tree` tests were run, and
the source was restored:

| Mutant | Deletion applied | Result | Verdict |
| --- | --- | --- | --- |
| A `error` | `error: Some(message)` from the byte-bounded first-pass error literal | `FAILED. 26 passed; 1 failed` | **killed** |
| B `cancelled` | `cancelled: true` from the cancellation literal | `FAILED. 25 passed; 2 failed` | **killed** (both cancellation tests) |
| B `examined_entries` | `examined_entries` from the cancellation literal | `FAILED. 26 passed; 1 failed` | **killed** |
| A `examined_entries` | `examined_entries` from the **error** literal | `ok. 27 passed; 0 failed` | **survives — equivalence proven** |
| — | source restored | `ok. 27 passed; 0 failed` | baseline clean |

The fourth row is the important one: it **proves** the reachability argument in §2
rather than asserting it. Deleting the counter on the error path changes nothing
observable, because `openat` fails before any entry is visited and the field is
already `0` — which is exactly what `Default` supplies. The same argument covers the
remaining seven by construction, and the assertions added in §3 will fail if it ever
stops holding.

### Population summary

| Population | Generated | Killed by this change | Already killed | Equivalent (no seam) |
| --- | --- | --- | --- | --- |
| field-deletion mutants in `services/file_tree.rs` | **12** | **3** | 1 | **8** |

The whole file generates **66** mutants; the 12 above are the field-deletion subset
that constitutes this file's share of the 34-mutant unfilterable floor.

**Net effect on the inherited handoff**: 11 untriaged survivors → **8 survivors, each
with a proven equivalence argument and a named durable fix**. Reported as **baseline,
not as regressions introduced by this change**, exactly as the amendment requires.
