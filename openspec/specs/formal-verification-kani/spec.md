# formal-verification-kani Specification

## Purpose
Establish Kani as LushText's single formal verification tool and define where harnesses live, how the Kani lane runs locally and in sharded CI, how safety regressions are pinned with should_panic harnesses, and how proved properties state their input domain.

## Requirements

### Requirement: Kani is the single formal verification tool
The project SHALL use Kani as its only **maintained** formal verification
tool. Maintained formal properties SHALL be expressed as `#[kani::proof]`
harnesses over the Rust code that ships, or over pure decision cores that
production code drives. They MUST NOT be expressed as separate models in
another specification language. The programme record MUST record any
exception as a maintainer decision.

A **disposable evaluation model** in another language or tool is allowed only
when all of the following hold:
- a recorded maintainer decision authorises it for a named evaluation;
- it lives in a non-production location that no build, test, lint, policy,
  audit, or CI gate reads;
- no production crate depends on it;
- no claim in rustdoc, specs, or the programme record rests on it alone.

An evaluation model is evidence for a tool decision, not a maintained
property.

#### Scenario: A new formal property is added
- **WHEN** a contributor adds a machine-checked property that the project maintains
- **THEN** it is a Kani harness in the crate that owns the checked code
- **AND** it checks production code or a production-driven pure core, not a re-implementation

#### Scenario: An evaluation model stays outside every gate
- **WHEN** an authorised evaluation adds Quint, TLA+, or other non-Kani models
- **THEN** they live under the evaluation's non-production location, for example `formal/evaluation/`
- **AND** `make check`, `make test`, `make kani`, the Cargo workspace, `cargo deny`, the Sonar scope, and every CI workflow pass unchanged without the evaluation's tools installed

### Requirement: Harnesses live beside the checked code behind cfg(kani)
Harnesses SHALL live in a `#[cfg(kani)]` module of the crate that owns the
checked code. That code SHALL be GTK-free and I/O-free. The workspace SHALL
declare `cfg(kani)` as an expected cfg, so that ordinary builds and Clippy
emit no `unexpected_cfgs` warning. Ordinary `cargo build`, `cargo test`, and
the stable-toolchain gates MUST NOT depend on Kani being installed.

#### Scenario: Ordinary gates without Kani
- **WHEN** `make check` and `make test` run on a machine without Kani
- **THEN** they pass and compile no harness code

### Requirement: Harness modules are verification code, not production code
A Kani harness module SHALL be named `kani_proofs.rs` and declared by its parent
module as `#[cfg(kani)] mod kani_proofs;`. Such a module is verification code.
The mutation scope SHALL exclude it: no ordinary build compiles it, so no test
can kill a mutant inside it. The workflow-boundaries gate SHALL recognise it as
a verification module, not as undeclared decision logic or as an undeclared
module in a role home, but only when its parent declares it under `cfg(kani)`.
Any `kani_proofs.rs` under `crates/`, in `ui/` or not, whose parent module
does not declare it under `cfg(kani)`, or that has no parent module file,
SHALL fail the gate.

#### Scenario: Harness code generates no mutants
- **WHEN** the configured mutation scope is listed
- **THEN** no mutant is listed in any `kani_proofs.rs` file

#### Scenario: A harness over a ui policy passes the boundaries gate
- **WHEN** a `ui/**/policy.rs` module declares `#[cfg(kani)] mod kani_proofs;` and the harness file sits in its child directory
- **THEN** `make check-workflow-boundaries` reports no undeclared-module or unclassified-decision-logic finding for that file

#### Scenario: An ungated harness module is caught
- **WHEN** a parent declares `mod kani_proofs;` without `#[cfg(kani)]`, or a `kani_proofs.rs` has no parent module file
- **THEN** `make check-workflow-boundaries` fails

### Requirement: The Kani lane has a local command and a bounded CI job
The project SHALL provide `make kani` to run every harness, and a scheduled or
manually dispatched CI workflow that runs the same harnesses. When the whole
lane would exceed the repository's 30-minute job budget, the workflow SHALL
split it into shards, one job each and each within the budget. The shards
SHALL come from a single table, which assigns every harness to exactly one
shard and is checked by the local policy gate. The table SHALL also give each
shard a gate: `pull-request` or `scheduled`; any other gate SHALL fail the
check. The same workflow SHALL run on pull requests and on pushes to `main`,
and those events SHALL run only the shards gated `pull-request`. Scheduled and
manually dispatched runs SHALL run every shard. The workflow SHALL build its
matrix from the table only after re-running the table check, so a table that
fails the check runs no shard. A shard MAY be gated `pull-request` only when
its measured runner wall time is at most 15 minutes. Each harness SHALL declare the unwind
bounds it needs. The lane SHALL report per-harness results.

#### Scenario: Local run
- **WHEN** a maintainer runs `make kani` with Kani installed
- **THEN** every harness is checked and the command fails if any harness fails

#### Scenario: CI run
- **WHEN** the Kani workflow runs on its schedule or by manual dispatch
- **THEN** it installs the pinned Kani version, runs every harness across its shards, and every job finishes within the job timeout

#### Scenario: Pull-request run
- **WHEN** a pull request is opened or updated, or a commit is pushed to `main`
- **THEN** the Kani workflow runs exactly the shards gated `pull-request` (initially `widgets-geometry`) and no other shard
- **AND** a failing harness in those shards fails the pull-request check

#### Scenario: A harness outside every shard is caught
- **WHEN** a new harness matches no shard, or more than one
- **THEN** the local policy gate fails before the workflow runs
- **AND** the workflow's shard-table job fails before any shard job starts

#### Scenario: A slow shard cannot join the pull-request gate
- **WHEN** a shard is gated `pull-request` but its recorded runner wall time exceeds 15 minutes
- **THEN** the local policy gate fails, and the workflow's shard-table job fails before any shard job starts

### Requirement: Kani shard budgets are measured on the CI runner and enforced with margin
Every shard in the Kani shard table SHALL record its wall time and peak
resident memory, measured on the CI runner the workflow uses. When a shard has
been measured by more than one run, the record SHALL keep the largest wall time
and the largest peak memory seen across those runs. The record SHALL also name
the runs it came from; timing from a developer machine does not count as a
measurement. The recorded wall time is the shard's `cargo kani` wall time
(crate build plus verification), not the whole job; the margin to the
30-minute job cap covers the job's setup around it. The lane's measurement mode
SHALL report, per shard, its wall time, its exit status, its peak resident
memory (the largest process in that shard's own process tree, not a figure
accumulated across earlier shards), and each harness's verdict and
verification time. It SHALL publish these figures as the job summary and as a
machine-readable artifact. Every CI shard job SHALL run in measurement mode.
The local policy gate, and the workflow's shard-table job, SHALL fail when a
shard has no recorded measurement or no named run, when its recorded wall time
exceeds 25 minutes, or when its recorded peak memory exceeds 12 GiB. Running a
shard SHALL NOT be refused for breaking a margin, so an over-margin shard can
still be re-measured. A shard that breaks a margin SHALL be split before any proof
bound is reduced. A bound that is reduced anyway SHALL be recorded in the
programme record, together with the property statement it weakens.

A change that adds or re-models a shard MAY carry a local figure in the table
while it is being developed, because the table must be complete before the
branch can be pushed and dispatched. That record SHALL say in its
`measured_in` text that it is pending dispatch and is a local figure, not a
runner measurement, and SHALL name the open change that owes its dispatch.
The local policy gate SHALL fail when such a record names no owning change,
or names one that is no longer an active change (archived or absent), so a
change cannot be archived while it still owes a figure. Before the change is archived, the local figure SHALL be
replaced by the larger figures of two `workflow_dispatch` runs of the Kani
workflow on the change's final form, naming both run ids. A dispatch re-measures
every shard: a shard whose dispatched figure exceeds its recorded one SHALL be
raised to it, naming the runs, and a recorded figure SHALL NOT be lowered by a
later run. The programme record SHALL list every shard whose table entry is
still pending, with the change that owes its dispatch.

#### Scenario: A dispatched run reports its budget
- **WHEN** the Kani workflow runs a shard job, on any event
- **THEN** the job summary lists the shard's wall time, exit status, peak resident memory, and every harness's verdict and verification time
- **AND** the same figures are uploaded as a per-shard JSON artifact

#### Scenario: An unmeasured shard is caught
- **WHEN** a shard is added to the table with no recorded wall time, peak memory or `measured_in` text (neither a runner measurement nor a local figure marked pending dispatch)
- **THEN** the local policy gate fails

#### Scenario: A shard near the runner's memory is caught
- **WHEN** a shard's recorded peak memory exceeds 12 GiB
- **THEN** the local policy gate fails until the shard is split or its recorded measurement is back within the margin

#### Scenario: A local figure is replaced before the change is archived
- **WHEN** a change adds a shard and records its budget from a local `make kani KANI_MEASURE` run
- **THEN** the record's `measured_in` says it is pending dispatch
- **AND** before the change is archived, two dispatched Kani workflow runs replace it with the larger of their figures and both run ids

#### Scenario: A pending figure outlives its change
- **WHEN** a shard's `measured_in` marks a local figure pending dispatch and names no owning change, or names a change that has been archived
- **THEN** the local policy gate fails
- **AND** the same record owed by a change that is still active passes

#### Scenario: A dispatch raises an older shard's figure
- **WHEN** a dispatched run measures an already-measured shard above its recorded wall time or peak memory
- **THEN** the record is raised to the larger value and names that run, and it is never lowered by a smaller later run

### Requirement: Safety regressions are pinned by should_panic harnesses
For each ordering or guard whose removal is known to break a safety property,
the project SHALL keep a `#[kani::should_panic]` harness showing that the
property fails without that ordering or guard. The canonical example is
renaming before the temp-file sync.

#### Scenario: Removing the temp sync is caught
- **WHEN** the durable-write protocol harness runs the sequence "write temp, rename" without the temp sync
- **THEN** Kani finds the torn-target counterexample and the should_panic harness passes

### Requirement: Proved properties state their input domain
A harness that proves a property only on a restricted domain SHALL state that
domain in the harness, and in the checked function's rustdoc or spec. An
example is whole-pixel `i32` inputs rather than arbitrary `f64`. A rustdoc
promise MUST NOT claim a broader domain than the one proved.

#### Scenario: Slice containment domain
- **WHEN** the slice containment property is documented
- **THEN** its rustdoc and spec state the whole-pixel domain on which Kani proves it

### Requirement: External modelling tools are adopted only on recorded empirical evidence
The project SHALL adopt an external modelling, model-checking, or deductive
verification tool only after a written comparison on this project's own
targets, and only through an explicit decision section in that comparison.
This covers Quint, TLA+ and its checkers, `stateright`, deductive verifiers
for Rust such as Verus, and any other tool in addition to or instead of Kani,
whether it is adopted as a maintained lane, a design-sketch tool, or a test
driver.

The comparison SHALL record, for each tool and each target:
- modelling effort;
- check time, memory, and state counts;
- which safety, liveness, and fairness properties were actually exercised;
- whether known answers were reproduced;
- install and CI cost against the repository's 30-minute job cap;
- bridge options back to Rust.

For a deductive verifier it SHALL also record:
- the ratio of specification, proof, and invariant lines to verified code
  lines;
- the rewrite footprint the verified subset imposes on the production module;
- the trusted base: every unverified specification the proof relies on;
- how a production change propagates to the proof.

Exploratory attempts SHALL be recorded, including those that failed. Until
such a decision says otherwise, Kani SHALL remain the single maintained
formal tool. A recommended lane, dependency, or maintained model SHALL be
carried by a separate follow-up change.

#### Scenario: A tool is proposed on opinion alone
- **WHEN** a change proposes a maintained lane or a production dependency for a non-Kani modelling tool
- **AND** no recorded comparison with a decision section supports that adoption
- **THEN** the change is not accepted until such a comparison exists

#### Scenario: The evaluation calibrates against known answers
- **WHEN** a tool is evaluated on a target whose answer Kani already established, such as durable-write crash atomicity and the torn destination when the temp sync is skipped
- **THEN** the comparison records whether the tool reproduced each known answer
- **AND** a tool that fails to reproduce a known answer is not adopted on the strength of that target

#### Scenario: The decision keeps Kani only
- **WHEN** the comparison's decision section chooses to keep Kani only
- **THEN** no external tool gains a CI lane, dependency, or maintained model
- **AND** the evaluation models remain disposable evidence in their non-production location, or are removed, as the decision states

#### Scenario: A deductive verifier's cost is recorded
- **WHEN** a deductive verifier such as Verus is evaluated on a production pure core
- **THEN** the comparison records its annotation ratio, rewrite footprint, trusted base, and proof-maintenance cost next to its verification results
- **AND** an unbounded proof counts as evidence only if it fails on at least two real pre-fix defects that a Kani harness also rejects
- **AND** a proof that relies on a trusted specification for a decision under test does not count as evidence for that decision
- **AND** a known-false claim counts as reproduced only when the verifier proves a concrete witness against it, and a mutant counts as caught only when the verifier rejects the named invariant obligation, never on a timeout, a resource limit, or a build error

### Requirement: Unbounded claims are attempted in Kani first
Before any tool other than Kani is discussed for a claim that needs to be
unbounded, the project SHALL make two recorded attempts inside Kani:

1. Kani loop contracts, or the equivalent one-step inductive harness: an
   arbitrary state assumed to satisfy the invariant, one step, and the
   invariant asserted after it, together with a base case that the initial
   state satisfies the invariant and a cover showing the assumed invariant
   is satisfiable.
2. A compositional argument: small harnesses plus a written proof note that
   extends a bounded proof to any size.

A tool other than Kani, including Lean, SHALL be re-discussed only if both
attempts fail **and** an unbounded claim is actually needed, for example as
evidence for GTK Lush publication. It SHALL then be decided by a recorded
maintainer decision. The programme record's tool section and its candidate
list SHALL state this gate.

A recorded maintainer decision MAY authorise a time-boxed, disposable
evaluation that measures another tool on an unbounded claim before the claim
is needed. The evaluation SHALL record both Kani attempts for each claim it
targets, next to the other tool's result: the attempts already on record, or
new ones made during the evaluation, made before the other tool's attempt. It
SHALL NOT target a claim that either Kani attempt already verified; where a
Kani attempt verifies part of a claim (for example any number of actions at a
fixed number of ids and windows), the evaluation SHALL target only the part
that remains. Its result SHALL be a recommendation recorded in the programme
record, and SHALL NOT lead to adoption for a claim unless both Kani attempts
for that claim failed and the claim is needed.

#### Scenario: A claim needs to be unbounded
- **WHEN** a contributor needs an unbounded version of a bounded Kani result
- **THEN** a loop-contract harness (or its one-step inductive equivalent, with its base case and satisfiability cover) and a compositional argument are attempted and their outcomes recorded before another tool is proposed

#### Scenario: Both attempts succeed
- **WHEN** either attempt verifies the claim
- **THEN** the claim is recorded as proved in Kani with its stated domain, and no other tool is proposed for it

#### Scenario: Lean note reflects the gate
- **WHEN** this change completes
- **THEN** the "Lean is dormant" text in `docs/next/formal-verification.md` and the "Dormant: Lean" section of `docs/next/formal-verification-next.md` say Lean is re-discussed only if both attempts fail and an unbounded claim is needed

#### Scenario: An authorised evaluation measures a tool before the need
- **WHEN** a maintainer-authorised disposable evaluation measures a tool other than Kani on an unbounded claim that no trigger yet needs
- **THEN** its report records both Kani attempts for that claim next to the other tool's result
- **AND** it does not target a claim that a Kani attempt already verified, such as the slice loop within its per-bin domain
- **AND** its decision adopts nothing for that claim unless both Kani attempts failed and the claim is needed

#### Scenario: A Kani attempt verifies part of an evaluated claim
- **WHEN** an authorised evaluation's Kani inductive-step attempt proves a journal invariant for any number of actions at the lane's id and window scope
- **THEN** the report records that bound as removed in Kani, with the harness text and its time and memory
- **AND** the other tool targets only the remaining generality, such as any number of ids and windows

### Requirement: Experimental Kani features are declared per shard
A harness that needs an experimental Kani feature SHALL be enabled only
through its shard's entry in the single shard table. Loop contracts
(`-Z loop-contracts`) and function contracts are examples. `make kani` and
the CI workflow SHALL pass those flags from the table. Harnesses in other
shards SHALL NOT receive them. The table's policy check SHALL still assign
every harness to exactly one shard.

#### Scenario: A loop-contract harness runs in its shard
- **WHEN** the shard containing a loop-contract harness runs locally or in CI
- **THEN** Kani is invoked with the loop-contract flag from the table, and the job stays within the 30-minute cap

#### Scenario: Flags do not leak
- **WHEN** any other shard runs
- **THEN** its Kani invocation carries no experimental flag it did not declare

### Requirement: Journal harnesses distinguish window actors from process actors
The draft-journal Kani model SHALL represent a **window actor** and a
**process actor** as distinct things. Windows of one process SHALL share the
model disk, the process-wide manifest write lock, the process-local target
write guard, and any process-wide journal coordinator. Each window SHALL keep
its own editors, manifest copy, authority, restore holds, tombstones, and
in-flight flags. Processes SHALL share only the disk. The two-process harness
that pins axiom A6 (`a_second_writer_breaks_the_journal_invariants`) SHALL stay
a `should_panic` harness, using separate process actors, until the programme
record changes the A6 decision. The multi-window harness SHALL be assigned to
exactly one shard of the Kani shard table.

#### Scenario: The multi-window harness shares process-wide state
- **WHEN** the two-window harness runs
- **THEN** both window actors observe one manifest lock, one target guard, and one coordinator state, and each keeps its own per-window state

#### Scenario: The A6 pin is unchanged
- **WHEN** the Kani lane runs after this change
- **THEN** `a_second_writer_breaks_the_journal_invariants` still finds a counterexample and passes as `should_panic`
- **AND** `make check-kani-shards` reports every harness, including the new one, in exactly one shard

### Requirement: Proof strength is measured by mutation
Every module whose behaviour a Kani harness is claimed to check SHALL appear
in the Kani oracle table with the ordered list of harnesses that check it, and
the policy gate SHALL fail when a listed harness does not exist or a listed
module has no harness. The programme record SHALL hold, for each such module,
the latest measured mutant count and kill counts under the tests oracle, the
Kani oracle, and both, with the source revision and tool versions measured.
Every mutant that survives the Kani oracle, including timeouts, SHALL be
recorded with exactly one triage class: **add harness/assertion** (the harness
or assertion is added and the mutant is shown killed), **equivalent mutant**
(with the reasoning that no observable behaviour of the module changes), or
**accepted gap** (with the reason, such as outside the proved input domain or
constrained only by tests). A harness added for a survivor SHALL be assigned to
exactly one shard and SHALL keep that shard within the CI job budget.

#### Scenario: A new harness module enters the oracle table
- **WHEN** a contributor adds a Kani harness for a module not yet in the oracle table
- **THEN** `make check-kani-shards` fails until the module and its harnesses are listed

#### Scenario: A survivor is triaged
- **WHEN** a mutant of an oracle module survives every harness in its list
- **THEN** the programme record names that mutant and gives it exactly one triage class with its justification

#### Scenario: An added assertion kills its survivor
- **WHEN** a survivor is triaged as add harness/assertion
- **THEN** the change that adds the assertion re-runs the Kani oracle for that mutant and records it as killed
- **AND** the unmutated harness still verifies within its shard budget

#### Scenario: A survivor exposes a defect
- **WHEN** triage of a survivor shows that the shipped code violates its documented contract
- **THEN** the defect is fixed in the same change behind a test that failed before the fix

### Requirement: The Replace All per-file journal protocol is proved
Kani shard `core-replace-protocol` SHALL prove the I/O-free protocols of `services/content_search/replace/protocol.rs` over every outcome sequence and crash point, composed with an abstract durable write that assumes the existing write-protocol theorems.

#### Scenario: A crash at any point
- **WHEN** the process stops at any step of an apply, an Undo or a cleanup
- **THEN** the proof shows the target holds its original or its replaced bytes, and replaced implies an active entry whose body holds the original

#### Scenario: Undo over a diverged file
- **WHEN** the current file's digest equals neither the original nor the replaced digest
- **THEN** the proof shows Undo writes nothing to it

#### Scenario: A reordered cleanup is caught
- **WHEN** a mutation makes cleanup delete a body before its entry
- **THEN** `core-replace-protocol` reports a counterexample in which an entry references a missing body

### Requirement: The Replace All stream's UTF-8 seams are proved split-independent
Kani shard `core-replace-stream` SHALL prove that, fed in chunks of 1 to 4 bytes, the stream's incremental UTF-8 carry rejects exactly the sources of at most 8 arbitrary bytes that one validation of the whole source rejects, and that the byte-loop validation the harness runs on equals `core::str::from_utf8` on every input of at most 4 bytes. Both new shards SHALL have measured runner budgets in the shard table, and `write_protocol.rs` and its shards SHALL be unchanged.

#### Scenario: A character split across chunks
- **WHEN** a chunk boundary falls inside a multi-byte character
- **THEN** the proof shows the UTF-8 verdict is the one-shot verdict

### Requirement: The Replace All engine's line logic is checked exhaustively over a bounded domain
Because no Kani form of the engine that was tried fitted the shard margin (design D11, "Kani memory"), a unit test SHALL check the engine at every point of bounded domains. On a valid UTF-8 source, the chunked feed's outcome SHALL equal the whole feed's, and a replaced file's output, count and digests SHALL equal the whole feed's and the reference builder's. A source that is not valid UTF-8 SHALL never be replaced, at any chunk size.

#### Scenario: The bounded domain
- **WHEN** the check runs
- **THEN** it covers every source of at most 4 characters over `a`, `b`, `\r`, `\n` and `é`, every edit set of at most two edits over small line and replacement tables that include a recorded line holding `é`, every chunk size, and an unlimited and a binding output limit
- **AND** each case the retired engine harness covered (two edits applied, the limit stopping a file, a stale line, an unterminated final line replaced across a seam, a cut character inside a replaced targeted line) is reached at least once

#### Scenario: A CRLF split across chunks
- **WHEN** a chunk boundary falls between the `\r` and `\n` of a targeted line
- **THEN** the bounded check covers it and the streamed result equals the reference

#### Scenario: A mishandled seam is caught
- **WHEN** a mutant of the engine drops the earlier part of a targeted line split across chunks
- **THEN** the bounded check fails on the chunked feed's outcome

#### Scenario: An invalid source is never replaced
- **WHEN** the check feeds every source of at most 3 symbols over the valid alphabet plus the bytes `0x80`, `0xC3` and `0xFF`, with no edit and every single edit of the same tables, every chunk size and the same limits
- **THEN** no source that is not valid UTF-8 is `Replaced` at any chunk size, it ends `NotUtf8`, `StaleLine` or `OutputLimitExceeded`, and with no edit and no limit it ends `NotUtf8`
- **AND** an invalid source split by a seam is reached at least once
- **AND** a mutant whose end of stream skips the UTF-8 carry's final check fails the check
