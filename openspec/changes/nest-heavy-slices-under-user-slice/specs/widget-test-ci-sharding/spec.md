## MODIFIED Requirements

### Requirement: Parallel local shards are isolated from each other
In a parallel local run each shard SHALL have its own admitted memory scope (`scripts/with-memory-cap.sh`, in the host-wide heavy slice, capped by `WIDGET_SHARD_MEMORY_MAX`), temporary-file scope (`--session-runtime`), D-Bus session, headless compositor, `XDG_RUNTIME_DIR` and Wayland socket. It SHALL refuse a Wayland socket path over 100 bytes, wait for admission, saying so, rather than start uncapped, and fail with `TEMP-LEAK:` on a leak or `OOM-KILL:` on any OOM kill, even after a passing retry.

#### Scenario: Shards do not share a session
- **WHEN** a parallel local run is in flight
- **THEN** each shard's compositor listens on a socket inside its own runtime directory, and no two shards share a runtime directory, bus, or memory scope

#### Scenario: Overlong socket path is refused
- **WHEN** the resolved temporary directory would make a shard's Wayland socket path longer than 100 bytes
- **THEN** each shard fails before its session starts, the message names the temporary directory as the cause, and the run fails

#### Scenario: A leak is attributed to its shard
- **WHEN** a test in one shard leaves a temporary entry in its scope
- **THEN** that shard's scope reports `TEMP-LEAK:`, the summary marks that shard failed, and the run fails

#### Scenario: An out-of-memory kill is never green
- **WHEN** a process in one shard's memory scope is OOM-killed and the harness's retry then passes the test
- **THEN** that shard reports `OOM-KILL:`, the summary marks it failed, and the run fails

#### Scenario: Interrupted run leaves nothing behind
- **WHEN** a developer interrupts a parallel run with Ctrl-C
- **THEN** every shard's session is stopped and its runtime directory and scope are removed

#### Scenario: Shard scopes nest under the user manager's user.slice
- **WHEN** a parallel local run admits a shard's memory scope on a host with a systemd user manager
- **THEN** the scope's cgroup path lies under `user.slice/user-lushtext.slice/user-lushtext-heavy.slice`
