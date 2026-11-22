# Self-test wall times (task 2.2, design D7)

Not timing verdicts: wall times of hook-time self-tests on the shared
workstation, with the load recorded. 2026-11-18 about 22:10, load 4.26
(one-minute).

| Self-test | Runs | Wall time | Budget / baseline |
| --- | --- | --- | --- |
| `scripts/bounded-log-scan.sh --self-test` | 3 | 1.32 s, 1.31 s, 1.28 s | under 5 s (D7) |
| `scripts/run-widget-tests.sh --self-test` | 2 | 5.84 s, 5.97 s | 2.7 s on main (2026-11-18) |

The widget runner's self-test now also runs the three capture cases (about
2 s, side by side: a 2^20-line flood, kill -9 and SIGTERM mid-flood) and the
stage's self-test (about 1.3 s, its cases side by side). `make
check-temp-scope` runs it in place of its direct `with-temp-scope.sh
--self-test`, so each self-test runs once per hook.

In the CI widget image (`lushtext-ci-widget:b582f78df5de`, Fedora 45, no
`pgrep`: the self-tests read `/proc` instead) the widget runner's self-test
passed in 5.3 s and left `/tmp` as it found it.
