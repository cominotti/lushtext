---
description: When to run Kani, proof strength, GTK axioms and timing campaigns — only what a change needs, at the point it is needed
---
<!-- global rule: loaded for every request -->

# Verification Runs

Kani shards, proof-strength mutation, GTK axiom samples and timing campaigns
cost minutes to hours each, and a local run that CI repeats buys nothing. This
rule decides **which** of them a change runs and **when**. How to run them
(the lock protocol, memory cap, shard table, budgets) stays in
[`build.md`](./build.md): the shared-machine section, "Formal Verification
(Kani)" and "GTK Axioms and Toolkit Updates". It was adopted on 2026-11-14,
after local Kani and lane runs of the long-line programme repeatedly cost
hours, collided with timing campaigns and were lost to agent interruptions.

## Kani

Find what a change touches before running anything. A harness "reads" a
module when it calls its code, directly or through a production function it
drives, so look both ways:

- `scripts/kani-shards.py oracle <module>` names the harnesses whose oracle is
  that module (it errors for a module that is no harness's oracle);
- `grep -rln '<module_name>' crates --include=kani_proofs.rs` finds the
  harness files that reach it through another proven module
  (`paragraph_boundary` is also driven by the `buffer_replacement`,
  `highlight_chunking` and `paragraph_profile_watch` harnesses, which `oracle`
  does not list);
- `scripts/kani-shards.py list` gives each harness's shard and gate.

Then:

- **Pull-request shards** (`gate="pull-request"`) run in CI on every pull
  request and push to `main`; those in the `main` ruleset are required to
  merge. Do not run them locally unless the change edits a harness or a module
  one of them reads, and then only the affected shard.
- **Scheduled shards** (`gate="scheduled"`) run only weekly or by dispatch, so
  a local run is the only evidence before merge. When a change edits a harness
  or a module a scheduled shard reads, run **that shard once**, after the code
  is final (not after every edit), detached so an agent interruption cannot
  kill it (`setsid nohup scripts/heavy.sh -- make kani KANI_SHARD=<shard>`;
  `heavy.sh` also waits for a running timing campaign).
- **No proven module touched:** the spec records "Kani: none (no proven
  module touched)" and no stage runs Kani. Never run every shard locally: a
  full `make kani` belongs to the weekly or dispatched CI lane. A GTK bump does
  not touch the proofs, and a Kani version bump is proved by that lane.
- **Review:** a reviewer reruns a shard only when its own fixes touched that
  shard's harness inputs.
- **Shard budgets:** a new or changed shard records its budget from two CI
  runs (`build.md`, "Shard gates and budgets"). For a pull-request shard the
  PR's own Kani run is one of them. Dispatch `kani.yml`, which runs every
  shard, only for a scheduled shard's figure, once the change's final code is
  pushed. A pending record names its owning change, so the archive cannot
  forget it.

## Proof strength

`make proof-strength` runs only when a change adds or changes harnesses (or
the properties they assert), or when a programme record asks for it. Shard its
Kani arm (`PROOF_STRENGTH_SHARD=k/n`) when the measured peak per CBMC fits the
memory budget, and run it detached.

## GTK axioms

- On a GTK, GtkSourceView, Pango, GLib or Libadwaita version change, run
  `make gtk-axioms` **first**, before anything else (`build.md`).
- Otherwise run only the samples of the axioms the change adds, edits or newly
  relies on (`make gtk-axiom-sample AXIOM=<id> CHECK=1`, or
  `scripts/run-gtk-axioms.sh runtimes --only <ids>` inside SDKs).
- On pull requests CI runs the probes on the shipped toolkit and on the
  GNOME 50 floor, and both jobs are required to merge. Both skip probes marked
  slow (`GTK_LUSH_AXIOMS_INCLUDE_SLOW=0`), so a change that adds or edits a
  slow probe runs its sample locally on both toolkits.

## Timing campaigns

- Every timed run goes through `scripts/measure.sh`, which takes the
  measurement lock and records the load and top processes before and after
  (`build.md`, the lock protocol and its load gate).
- An optimisation change measures the cases it affects: four rounds
  interleaved `main` → branch → `main` → branch. An environment change
  (toolkit or runtime) measures the full quick tier the same way.
- Extra rounds (attribution, repeats) run only when a verdict is ambiguous or
  a round is contaminated, and the reason is recorded.
- A campaign longer than about two hours runs detached from any agent, so a
  usage limit or an agent restart cannot kill it; the agent reads its results
  afterwards.
- Heavy work started through `scripts/heavy.sh` waits for a running campaign
  by itself; never bypass the helpers to start heavy work beside one.

## Checkpoints

Commit and push per task group during implementation, so work survives an
interruption. A resumed stage reads the branch, `tasks.md` and the handoff
directory before redoing anything.
