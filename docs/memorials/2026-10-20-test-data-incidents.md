# 2026-10-20: test data incidents

Why test isolation is a property of the test process and why every test lane
runs under `scripts/with-temp-scope.sh`. The rules are in
[`.agents/rules/testing.md`](../../.agents/rules/testing.md), "Temporary Files"
and "Test Isolation From Real App Data".

## A widget test overwrote the real session

On 2026-10-20 a widget test saved a session before calling
`ensure_gtk_init()`, which was then the only place that set
`LUSHTEXT_DATA_DIR`, and overwrote the developer's real `session.json`; the
contents were unrecoverable. Isolation is therefore established as the first
statement of the widget harness's `main`, before any test runs, and
`json_store::data_dir()` panics in test builds when it resolves into the real
home. `app::test_session_saved_before_gtk_init_lands_in_the_private_test_directory`
is the regression test.

## Temporary directories exhausted `/tmp`

A child-per-test harness multiplies a one-directory leak: one
`lushtext-test-<pid>` directory per widget child once accumulated 45,978
directories and exhausted `/tmp`'s tmpfs inode table (see
`crates/gtk-lush/proof-harness/CHANGELOG.md`). That is why the supervisor owns
one root per run and every lane fails with `TEMP-LEAK:` when anything is left
behind.

## The signal trap that raced the document portal

`with-temp-scope.sh` once exited from its Ctrl-C trap before the session's
services had released the runtime directory, which raced xdg-document-portal:
it recreated `doc/` after the removal. The wrapper now records the signal and
waits for the session to release the directory before removing the scope.
