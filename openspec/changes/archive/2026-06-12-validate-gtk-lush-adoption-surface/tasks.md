## 1. Phase Framing And Contracts

- [x] 1.1 Update `docs/next/gtk-lush.md` to split Phase 5 into adoption validation and later publication/graduation, keeping publication, repo split, LushText published dependencies, and upstreaming out of this phase.
- [x] 1.2 Update `crates/gtk-lush/GOVERNANCE.md` with a `validate-gtk-lush-adoption-surface` review entry template covering constitution answers, adoption evidence, API review, non-publication status, and specialist review lanes.
- [x] 1.3 Update canonical GTK Lush governance/workspace specs or sync notes as needed so `validate-gtk-lush-adoption-surface` is the required pre-publication phase before `graduate-and-publish-gtk-lush`.
- [x] 1.4 Define the committed locations for the adoption lab, stock starter fixtures, adoption matrix, timed journal, unrelated-project notes, generated artifact roots, and ignored temporary checkouts.
- [x] 1.5 Add or update ignore rules so generated adoption artifacts, large logs, screenshots, frame streams, and unrelated external project checkouts do not enter git accidentally.

## 2. Adoption Lab Workspace Consumer

- [x] 2.1 Add a workspace adoption-lab crate outside `crates/gtk-lush/`, such as `crates/gtk-lush-adoption-lab`, and wire it into the root workspace without making it a GTK Lush family crate.
- [x] 2.2 Add the lab dependencies on GTK, Libadwaita, and the GTK Lush crates it consumes, then run `cargo hakari generate` and review `Cargo.lock`, `workspace-hack`, and Flatpak cargo-source impact.
- [x] 2.3 Build the lab as a usable GTK application or gallery whose first screen is the working adoption surface, not a landing page.
- [x] 2.4 Implement a `gtk-lush-signals` lab workflow covering row or widget signal ownership, binding ownership, clear/drop behavior, and recycled/rebound state.
- [x] 2.5 Implement a `gtk-lush-settle` lab workflow covering debounce, settle-burst pending state, superseding timer re-arm, weak target cancellation, and visible latest-generation behavior.
- [x] 2.6 Implement a `gtk-lush-tasks` lab workflow covering bounded background work, main-thread completion, freshness or stale completion handling, panic-safe error reporting, and non-blocking UI controls.
- [x] 2.7 Implement a `gtk-lush-viewport` lab workflow covering scroll-adjustment page-size observation, rest-state or rest-pause behavior, lower-edge detection, and app-owned reaction logic.
- [x] 2.8 Implement a `gtk-lush-widgets` lab workflow covering `ClipBin` constrained geometry and `RenderHoldOverlay` capture, failed capture, warm/reveal, early clear, and non-targetable cover behavior.
- [x] 2.9 Implement a `gtk-lush-proof-harness` lab or test workflow showing a non-LushText GTK test registered and run through the harness with documented waits.
- [x] 2.10 Implement a `gtk-lush-proof-spine` lab workflow showing GTK-free readiness, blocker, snapshot, workflow-event, and artifact-envelope values produced by an app-owned provider.
- [x] 2.11 Ensure the lab UI covers no required context, representative populated state, many or awkward rows/items, constrained width/height, reachable commands, readable text, preserved fixed controls, and no unintended root scrollbars.
- [x] 2.12 Add adoption-lab unit, integration, or widget tests for the workflows whose behavior can be verified without a live visual run.

## 3. Adoption Matrix And Local Policy

- [x] 3.1 Create the crate-by-crate adoption matrix listing every functional GTK Lush crate, lab workflow, standalone example, stock fixture status, tests/proof evidence, friction status, API decision, and follow-up item.
- [x] 3.2 Add a deterministic matrix completeness check that fails when a functional crate is missing or lacks workflow, evidence, friction, or API-decision fields.
- [x] 3.3 Add a deterministic adoption-lab build/test target and make it discoverable through `make help` and build documentation.
- [x] 3.4 Add a stock fixture check target that validates checked fixtures use exactly one `gtk-lush-*` path dependency and no LushText crates.
- [x] 3.5 Update `scripts/check-gtk-lush-policy.py` or companion policy tooling so the adoption lab is not treated as a family crate, while `crates/gtk-lush/` leaf rules remain strict.
- [x] 3.6 Update Makefile variables, CI jobs, documentation, and any policy lists needed so adoption-lab and fixture checks are included in the phase verification ladder without requiring crates.io publication.

## 4. Stock Gtk-Rs Afternoon Adoption

- [x] 4.1 Choose the first timed-adoption crate, preferring the crate whose API most needs third-party adoption pressure.
- [x] 4.2 Create a stock gtk-rs starter-style fixture outside the workspace member graph that adopts exactly one GTK Lush crate through a path dependency.
- [x] 4.3 Run the timed adoption exercise from a fresh-session mindset and record start/end or elapsed time, commands, starter shape, code summary, friction, and resulting decisions.
- [x] 4.4 Classify every timed-adoption friction point as documentation, example, naming, type-shape, feature flag, missing helper, overreach, accepted limitation, or follow-up.
- [x] 4.5 Update the adoption matrix with the timed-adoption result and link each non-accepted friction item to an implementation task, doc change, or follow-up issue.
- [x] 4.6 Ensure the stock fixture check runs locally and does not require network access, external publication, LushText resources, LushText GSettings schemas, or another GTK Lush crate.

## 5. Unrelated Existing Project Spike

- [x] 5.1 Select a small public gtk-rs or Libadwaita project for the spike, recording candidate rationale, license compatibility, selected GTK Lush crate, and source version or commit.
- [x] 5.2 Attempt the adoption in a temporary external worktree or fork without committing the outside project source into this repository.
- [x] 5.3 Preserve a bounded external-adoption note with commands, elapsed effort when available, patch summary or branch reference, friction, and decision.
- [x] 5.4 Classify external-spike friction through the same adoption-review categories used for the lab and timed starter.
- [x] 5.5 Update the adoption matrix and governance notes with the external-spike result, accepted limitations, and any follow-up work.

## 6. Friction-Driven API Hardening

- [x] 6.1 Review all lab, timed-starter, and external-project friction in one API review pass before broad polish work.
- [x] 6.2 Apply accepted breaking `0.0.0` API changes that reduce consumer ceremony, remove LushText-shaped assumptions, improve naming, clarify ownership, or preserve the anti-framework constitution.
- [x] 6.3 Reject or redesign any API idea that would introduce a view DSL, component model, app state/message loop, custom runtime, Libadwaita replacement, or runtime dependency between family crates.
- [x] 6.4 Update LushText call sites, adoption-lab workflows, stock fixtures, examples, doctests, README snippets, CHANGELOG entries, and public API snapshots for every accepted API change.
- [x] 6.5 Add or strengthen unit, property, doctest, widget, or visual proof coverage for each API or behavior changed by adoption friction.
- [x] 6.6 Record accepted limitations and deferred follow-ups explicitly in the matrix, governance entry, or review notes.

## 7. Documentation And Proof-Tool Cleanup

- [x] 7.1 Clean `cargo-gtk-proof` README, source module docs, help text, and comments that still describe Rust live proof as staged, future, or Python-authoritative.
- [x] 7.2 Rename or sync the canonical `cargo-gtk-proof` requirement title from stable staged subcommands to stable proof subcommands, preserving compatibility where OpenSpec sync requires a separate rename step.
- [x] 7.3 Document any intentionally retained historical `rust-staged` fixture or serialized metadata names as compatibility data rather than current tool status.
- [x] 7.4 Update GTK Lush family README, per-crate READMEs, examples, and CHANGELOGs to describe adoption-lab evidence, `0.0.0` pre-publication status, and non-publication boundaries.
- [x] 7.5 Update root `README.md`, `AGENTS.md`, `.agents/rules/build.md`, and other rules or skill references if the workspace layout, test commands, or GTK Lush adoption guidance changes.
- [x] 7.6 Run `make check-agent-docs` if rules, skills, AGENTS guidance, filesystem policy, or build-command documentation changes.

## 8. Verification Gates

- [x] 8.1 Run `make check-gtk-lush-policy`.
- [x] 8.2 Run `make gtk-lush-doctests`.
- [x] 8.3 Run `make gtk-lush-examples`.
- [x] 8.4 Run `make gtk-lush-msrv`.
- [x] 8.5 Run `make gtk-lush-api-advisory` and inspect generated public API snapshots for intentional `0.0.0` changes.
- [x] 8.6 Run the new adoption-lab build/test target.
- [x] 8.7 Run the new stock fixture check target.
- [x] 8.8 Run the new adoption matrix completeness check.
- [x] 8.9 Run `make test-widget-headless` when lab workflows, GTK Lush widgets, proof harness behavior, or LushText widget consumers change.
- [x] 8.10 Run `make visual-geometry-smoke` and `make check-visual-proof-policy` when visual-sensitive files, rendered geometry, animation, `RenderHoldOverlay`, or visual proof behavior changes.
- [x] 8.11 Run `make check`.
- [x] 8.12 Run `git diff --check` before final review.
- [x] 8.13 Run `openspec validate validate-gtk-lush-adoption-surface --strict`, `openspec validate --changes --strict`, `openspec validate --specs --strict`, and `openspec validate --all --strict`.

## 9. Specialist Reviews And Archive Evidence

- [x] 9.1 Run or request GTK testing review for the lab tests, stock fixture shape, widget harness usage, and any headless-test timing assumptions.
- [x] 9.2 Run or request live GTK/debugging and GTK/Libadwaita contract review if the lab introduces visual, focus, allocation, capture, or headless-session behavior.
- [x] 9.3 Run or request GTK performance review for lab responsiveness, worker usage, CI runtime, large fixture avoidance, and any new policy scripts.
- [x] 9.4 Run or request data-safety/privacy review for adoption journals, external-project notes, generated artifacts, logs, screenshots, and proof summaries.
- [x] 9.5 Run or request Rust architecture review for crate boundaries, leaf-crate policy, adoption-lab consumer placement, CQS/ownership shape, and API hardening.
- [x] 9.6 Run or request comment-quality review for new public APIs, examples, adoption docs, and non-obvious GTK or proof-tool behavior.
- [x] 9.7 Fix all actionable specialist findings or document accepted non-blockers with rationale in review notes.
- [x] 9.8 Complete the governance entry with constitution checklist, adoption evidence, timed journal, external spike, API review summary, verification commands, and non-publication status.
- [x] 9.9 Confirm no functional GTK Lush crates were published, no `0.1.0` release was prepared, no repo split was performed, and LushText still uses workspace path dependencies.
- [x] 9.10 Prepare archive handoff notes identifying what the later `graduate-and-publish-gtk-lush` phase may cite and what publication-specific work remains.
