Rules for every task:

- **Isolation.** Never touch the real user Flatpak installation except in
  task 4.2, the single authorised `make flatpak-install`.
  - Every experiment and test uses a private `FLATPAK_USER_DIR` under its own
    `TMPDIR`, and asserts that before its first `flatpak` call.
  - Headless only. Never `make run`, never the real `~/.local/share/lushtext`
    or `~/.config`.
- **CPUs and heavy work.**
  - Use CPUs 8–23 (`taskset -c 8-23`).
  - Heavy commands (`make check`, `git commit` with its hook,
    `make flatpak-install`) go through `scripts/heavy.sh`, one command per
    call.
  - Never `pgrep -f`, never `find /`.
- **Evidence** goes in `.claude/handoffs/stable-flatpak-install-origin/`. The
  stage-1 experiment is committed as `evidence/origin-experiment.sh` and `evidence/origin-experiment.log`.
  The stage-2 review experiments are committed beside it, as
  `evidence/prepare-order-experiment.{sh,log}` and
  `evidence/empty-repo-experiment.{sh,log}`.
- **Failing first** means the test is committed, or recorded red against
  `main`'s scripts, before the code that turns it green.

Scope statements:

- Tests: shell fixture tests only (no Rust behaviour changes). Rust unit,
  widget and property suites are not affected.
- Kani: none (no proven module touched).
- GTK axioms: none (no GTK, Libadwaita, GtkSourceView or Pango behaviour
  relied on). The flatpak and flatpak-builder behaviour is pinned by the
  real-`flatpak` fixture test (design, "Toolkit behaviour relied on").
- Timing: none (no timed path touched).
- Lanes: no smoke lane case changes. `make verify-flatpak-identity` is the
  installed-state lane, run once in 4.2.

## 1. Failing-first tests

- [x] 1.1 Write `scripts/test-flatpak-install-origin.sh` (design D7).
  - Use the private `TMPDIR`, `HOME`, `FLATPAK_USER_DIR`, `XDG_DATA_HOME`,
    `XDG_CACHE_HOME`, `XDG_CONFIG_HOME` and `LUSHTEXT_FLATPAK_INSTALL_REPO`,
    with an isolation assertion and an `EXIT`/`INT`/`TERM` cleanup trap.
  - Use the `org.test.lushtext` runtime fixture from `origin-experiment.sh`.
    Start from the maintainer's state: a disabled `lushtext-origin` whose
    repository was deleted.
  - Drive every install through `flatpak-install-origin.sh install <repo>
    <ref> -- <export command>`. The export command is `flatpak build-export`,
    or `false` for the failed build.
  - Print `SKIP:` and exit 0 without `flatpak`, `ostree` or `flock`.
  - Cover cases 1–7 of D7, including the failed build (3), the lock (6) and
    the character-set refusal (7).
  - Run it against `main` (no `scripts/flatpak-install-origin.sh`) and record
    in `runs.log` that it fails at case 1, where the guard is missing. Record
    too that case 1's `remote-ls --updates` baseline assertion already holds.
- [x] 1.2 Extend `scripts/test-flatpak-identity-verifier.sh`.
  - The stub `flatpak` answers `remotes --user --show-disabled …` with no
    remotes in the existing cases.
  - A new case answers with a dangling `lushtext-origin` line and asserts
    that `verify-flatpak-identity.sh` fails with the guard's message.
  - Record the new case failing against `main`'s verifier.
- [x] 1.3 Add a standalone `make test-flatpak-install-origin` target, run
  under `$(TEMP_SCOPE)` (`testing.md`, Temporary Files). Do
  **not** add it to `check-policy` yet: the pre-commit hook runs
  `check-policy`, and the test is red until group 2.
- [x] 1.4 Commit the test scripts, the standalone target and the red record
  in `runs.log` (signed, conventional, staged by name), and push the branch.

## 2. Shared install origin

- [ ] 2.1 Write `scripts/flatpak-install-origin.sh` with these subcommands:
  - `path` (D2);
  - `install <repo> <ref> -- <export command…>` (D3, D4):
    - it validates the character set and creates the parent before locking;
    - it re-executes itself under `flock --close` (the internal `__locked`
      subcommand);
    - it then runs the export, `prepare`, the `flatpak install --reinstall`,
      the prune and `check --installed`;
    - it touches no remote when the export fails.
  - `prepare` (D3), callable on its own;
  - `check` (D6), with the warning for a valid non-shared origin;
  - `check --installed <repo> <ref>` (D5), comparing the deployed commit with
    `flatpak remote-info --user -c lushtext-origin <ref>`.

  Derive the app id from `LUSHTEXT_FLATPAK_APP_ID` (default
  `dev.cominotti.lushtext`) and the remote prefix by flatpak's rule. It must
  pass `shellcheck` and `bash -n`.
- [ ] 2.2 Change `flatpak-install` in the Makefile to one call, with
  `repo="$$(./scripts/flatpak-install-origin.sh path)"`:
  `./scripts/flatpak-install-origin.sh install "$$repo"
  dev.cominotti.lushtext/$$(flatpak --default-arch)/master --
  flatpak-builder $(FLATPAK_BUILDER_FLAGS) --repo="$$repo"
  $(FLATPAK_BUILD_DIR) $(FLATPAK_MANIFEST)`.
  - There is no `--install`: the script installs after the repair (D1, D3).
  - The Makefile takes no lock of its own (D4).

  `flatpak` and `flatpak-deps` stay unchanged. Add a comment saying why the
  origin is a single shared path (D1, D2, D8).
- [ ] 2.3 Call `check` from `scripts/verify-flatpak-identity.sh` in a new
  "Flatpak install origin" section (D6).
- [ ] 2.4 Measure the wall time of `make test-flatpak-install-origin`
  through `scripts/measure.sh` and record it. At 10 s or less, add the target to `check-policy`. Above 10 s,
  run it from `test-flatpak-identity-verifier` instead (design D7).
- [ ] 2.5 Run `make test-flatpak-install-origin`,
  `make test-flatpak-identity-verifier`, `make check-flatpak-permissions` and
  `bash -n`/`shellcheck` on the new scripts. All must pass, and 1.1 and 1.2
  must turn green.
- [ ] 2.6 Commit and push.

## 3. Documentation (documentation.md)

- [ ] 3.1 `.agents/rules/build.md`: the Dev Builds bullet on
  `flatpak-install`, and the "Flatpak" section:
  - the shared repository path and override;
  - why it is shared and outside caches (worktree deletion,
    `build-cache-prune`, `clear-lushtext-xdg`, XDG cache semantics);
  - the repair and the post-check;
  - the guard and its one-line fix;
  - `make test-flatpak-install-origin`.
- [ ] 3.2 `README.md` "From This Checkout": the shared repository in one
  sentence, and the runtime corrected from 50 to 51 (pre-existing drift; the
  manifest says 51).
- [ ] 3.3 `.agents/skills/flatpak-rust/SKILL.md`: proportional validation
  names the guard and its test. Add a review checklist item: the install
  origin stays on the single shared path.
- [ ] 3.4 `docs/next/flatpak-packaging.md`: the install section, and the
  runtime/SDK corrected from 50 to 51.
- [ ] 3.5 `AGENTS.md`/`.claude/CLAUDE.md` Build Commands: add
  `make test-flatpak-install-origin` if the Makefile help lists it. The Rules
  Index entry for `build.md` stays as it is unless its summary needs the new
  topic.
- [ ] 3.6 Run `make check-agent-docs`, then commit and push.

## 4. Merge, final gates, and the single install

- [ ] 4.1 Merge `origin/main` (never rebase) and resolve `flatpak-install`
  against `reduce-build-disk-writes` if it has landed (D8).
  - Keep its `--state-dir`/`build-cache.sh checkout` and this change's
    `--repo`, lock, prune and check.
  - Run its `build-cache.sh audit` if present.
  - Rerun 2.5.
- [ ] 4.2 Run the authorised `make flatpak-install` once on the final tree,
  through `scripts/heavy.sh`. The maintainer's machine starts from the
  disabled dangling `lushtext-origin`. Record in `runs.log`:
  - that the remote stayed disabled and unchanged while the build ran, and
    the `prepare` line repointing it after the export;
  - `flatpak remotes --user -d --show-disabled` before and after;
  - `flatpak remote-ls --user --updates` exiting 0;
  - `flatpak info --user dev.cominotti.lushtext//master` (origin, commit)
    against `flatpak remote-info --user -c lushtext-origin <ref>`;
  - the repository's size (`du -sh`).
- [ ] 4.3 Run `make verify-flatpak-identity` and record that the new "Flatpak
  install origin" section passes.
- [ ] 4.4 Run `make check` through `scripts/heavy.sh`. Then run `openspec
  validate stable-flatpak-install-origin --strict`. Commit and push.
