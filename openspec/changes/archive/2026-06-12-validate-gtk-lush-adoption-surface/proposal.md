## Why

GTK Lush now has functional in-tree crates and Rust-owned visual proof tooling, but the program's real product is still unproven: a stock gtk-rs application must be able to adopt one crate in an afternoon without becoming a LushText-shaped app. Before any publication or repository split, we need one broad adoption-validation phase that exercises the whole family, logs friction, and reshapes APIs while they are still `0.0.0`.

## What Changes

- Add an in-tree GTK Lush adoption lab: a second-consumer application or gallery/demo maintained outside the GTK Lush family crates and outside the LushText app code, using every functional GTK Lush crate in realistic workflows.
- Add an adoption matrix that maps each crate to the workflow, example, docs, tests, and proof evidence that demonstrate independent adoption.
- Run and preserve a timed afternoon-adoption exercise in a stock gtk-rs starter-style fixture for at least one crate, with friction recorded as actionable follow-up items.
- Run an unrelated-existing-project adoption spike for at least one crate, recording the candidate, branch or patch summary, friction, and decision without publishing or vendoring the outside project into this repository.
- Drive an API review pass from adoption-lab and timed-test friction, including breaking pre-publication API improvements when they make adoption smaller, clearer, or more stock gtk-rs-compatible.
- Strengthen examples, READMEs, doctests, widget tests, proof fixtures, and policy checks so every crate has both single-crate examples and cross-family adoption evidence.
- Clean stale Phase 4 proof-tool wording that still describes `cargo-gtk-proof` as staged or Python-authoritative now that Rust proof parity is complete.
- Update `docs/next/gtk-lush.md` and governance docs to split Phase 5 into a non-publication adoption-validation phase followed by a later publication/graduation phase.
- Explicitly exclude crates.io functional publication, `0.1.0` release work, repository split, LushText migration to published versions, and upstreaming.

## Capabilities

### New Capabilities
- `gtk-lush-adoption-validation`: Cross-family adoption proof, including the second-consumer adoption lab, crate-by-crate adoption matrix, timed stock gtk-rs adoption journal, unrelated-existing-project adoption spike, friction-driven API review, and non-publication exit gates.

### Modified Capabilities
- `gtk-lush-program-governance`: Split the Phase 5 roadmap into adoption validation before publishing, define the non-publication gate, and require friction-driven review before any later `0.1.0` work.
- `gtk-lush-workspace`: Define where adoption-lab consumers, stock gtk-rs fixtures, friction journals, and bounded adoption artifacts live in the workspace without making them family crates.
- `cargo-gtk-proof`: Update the canonical proof-tool contract and wording so Rust live visual proof is no longer described as staged or future, while Python remains only an explicit diagnostic/oracle path.

## Impact

- **Workspace structure**: Adds a maintained adoption-lab consumer outside `crates/gtk-lush/`, plus bounded adoption fixtures, external-adoption notes, and journals in reviewable locations.
- **GTK Lush public APIs**: May make breaking pre-publication API changes across `gtk-lush-signals`, `gtk-lush-settle`, `gtk-lush-tasks`, `gtk-lush-viewport`, `gtk-lush-widgets`, `gtk-lush-proof-harness`, and `gtk-lush-proof-spine` when adoption evidence proves the current surface is awkward.
- **Examples and docs**: Updates per-crate examples, READMEs, CHANGELOGs, crate docs, `docs/next/gtk-lush.md`, and proof-tool docs/comments where adoption or parity wording has drifted.
- **Testing and policy**: Extends GTK Lush family gates with adoption-lab builds/tests, stock-fixture adoption checks, API advisory review, doctests, examples, widget/proof evidence where relevant, and the full LushText phase gate at completion.
- **Non-goals**: No functional crates.io publishing, no `0.1.0`, no separate `gtk-lush` repository, no LushText dependency switch to published crates, and no upstreaming round.
