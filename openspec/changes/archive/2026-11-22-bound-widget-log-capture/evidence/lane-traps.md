# Lane scripts' bounded temp files (task 3.5, design D5)

What ran, on the tree of commit `3af74179` (each inside `make check-policy`,
which the pre-commit hook ran on that commit, and again in
`scripts/heavy.sh -- make check-policy` before it):

- `shellcheck -x scripts/run-snap-smoke.sh scripts/ci-image-widget-test.sh
  scripts/blueprint-templates.sh`: clean, after fixing their pre-existing
  findings (SC1007, SC2295 in `blueprint-templates.sh`; SC2012 in
  `run-snap-smoke.sh`).
- `make check-blueprint` (`./scripts/blueprint-templates.sh check`,
  blueprint-compiler 0.22.2): passed, through the new trap.
- `./scripts/ci-image-widget-test.sh --self-test`: passed (inside
  `make check-agent-helpers`).

**The snap lane is checked by review only.** This host has no snapd, so
`run-snap-smoke.sh` did not run. Two of its edits have therefore never
executed:

- the app log moves to `$WORK_DIR/app.log`, under the `WORK_DIR` whose
  `EXIT` trap (set right after `WORK_DIR` is created, before the log is
  first written) removes it with the snap;
- the newest `.snap` is picked with a `-nt` loop instead of `ls -t | head`
  (shellcheck SC2012). Reviewed for equivalence: both pick the most recently
  modified `*.snap` (the loop also handles names with spaces), and with no
  `.snap` both leave `SNAP_FILE` empty (main sent `ls`'s error to
  `/dev/null`), so the unchanged emptiness check fails the same way.
