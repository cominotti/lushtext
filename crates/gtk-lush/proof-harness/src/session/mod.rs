// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one implementation of a private headless test session.
//!
//! Every private headless session of a test, smoke, proof or diagnostic lane
//! starts here, through this library or the `gtk-lush-session` binary (which
//! LushText's `scripts/headless-session` shim builds and execs). A session is:
//!
//! 1. **The environment, before the bus.** `dbus-run-session` starts with the
//!    profile's environment already set, so services the bus activates later
//!    (`xdg-desktop-portal-gtk`, the AT-SPI launcher) inherit it: with a11y
//!    off, `GTK_A11Y=test` (or a caller's `none`) and `NO_AT_BRIDGE=1`; with
//!    portals off, `ADW_DISABLE_PORTAL=1`, `GDK_DEBUG=no-portals` and
//!    `GTK_USE_PORTAL=0`.
//! 2. **A private, monitored system bus.** A `dbus-daemon` with no service
//!    directory is exported as `DBUS_SYSTEM_BUS_ADDRESS`, so rtkit and
//!    bluetoothd cannot be reached from the session, and `dbus-monitor`
//!    records every message for the guard.
//! 3. **Quiet media**, when the profile asks: PipeWire and WirePlumber
//!    (`policy` profile, no hardware monitors) without module-rt, with
//!    helper-private config, state and data homes.
//! 4. **The command**, optionally under `mutter --headless` (the `run` and
//!    `compositor` stages).
//! 5. **The runtime guard**, at teardown while every service is alive: a pure
//!    verdict over what was observed (see [`verdict`]). A violation prints
//!    `SESSION-HYGIENE: <class>: <detail>` and exits with
//!    [`SESSION_HYGIENE_EXIT_CODE`], or keeps a failed command's own status.
//! 6. **Teardown** in reverse order.
//!
//! Profiles name the option bundles a lane uses ([`profiles`]).

pub mod observe;
pub mod profiles;
pub mod verdict;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{TEST_FAILURE_EXIT_CODE, UNSUPPORTED_HOST_EXIT_CODE, scratch};

use profiles::{A11y, Media, Portals, Profile, SystemBus, WIREPLUMBER_PROFILE};
use verdict::{AcquiredName, BusRecord, ConfigFacts, Observations, Violation, ViolationClass};

/// Exit status of a session whose command passed but whose guard failed.
///
/// Distinct from success, test failure (`101`), an unsupported host (`77`),
/// a sample's fixture-invalid status (`2`), `timeout`'s `124` and signal
/// deaths (`128 + n`), so runners can report it as its own failure class.
pub const SESSION_HYGIENE_EXIT_CODE: u8 = 93;

/// Command name of the session bus launcher.
pub const DBUS_RUN_SESSION_COMMAND: &str = "dbus-run-session";
/// Command name of the bus daemon (the private system bus).
pub const DBUS_DAEMON_COMMAND: &str = "dbus-daemon";
/// Command name of the bus monitor (Fedora `dbus-tools`).
pub const DBUS_MONITOR_COMMAND: &str = "dbus-monitor";
/// Command name of the GLib D-Bus client used for lookups.
pub const GDBUS_COMMAND: &str = "gdbus";
/// Command name of the compositor.
pub const MUTTER_COMMAND: &str = "mutter";
/// Command name of the PipeWire daemon.
pub const PIPEWIRE_COMMAND: &str = "pipewire";
/// Command name of the session manager.
pub const WIREPLUMBER_COMMAND: &str = "wireplumber";
/// Command name of the PipeWire object dump (Fedora `pipewire-utils`).
pub const PW_DUMP_COMMAND: &str = "pw-dump";

/// Environment variable naming the `gtk-lush-session` binary inside a session.
///
/// Code inside a session that needs a *nested*, fully private session (a GTK
/// axiom probe's control run) starts `$GTK_LUSH_SESSION_BIN run …` without a
/// Cargo dependency on this crate. Library callers do not export it.
pub const SESSION_BIN_ENV: &str = "GTK_LUSH_SESSION_BIN";
/// Marker set inside a session; `compositor` refuses to run without it.
pub const SESSION_ACTIVE_ENV: &str = "GTK_LUSH_SESSION_ACTIVE";
/// The effective profile name, exported into the session.
pub const SESSION_PROFILE_ENV: &str = "GTK_LUSH_SESSION_PROFILE";
/// The inner stage's plan; set only between the outer and inner stage.
const PLAN_ENV: &str = "GTK_LUSH_SESSION_PLAN";

/// The media daemons' XDG homes, every one helper-private.
///
/// PipeWire and WirePlumber read their fragments from the config home and
/// WirePlumber writes `default-nodes` and stream state into the state home;
/// before this helper, every screenshot case rewrote the user's
/// `~/.local/state/wireplumber`. `make check-session-hygiene` fails if one is
/// dropped from this list.
pub const DAEMON_PRIVATE_HOMES: [&str; 3] = ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME"];

const PIPEWIRE_FRAGMENT: &str = include_str!("pipewire-quiet.conf");
const WIREPLUMBER_FRAGMENT: &str = include_str!("wireplumber-quiet.conf");
const SYSTEM_BUS_CONFIG: &str = include_str!("system-bus.conf");
const FRAGMENT_FILE_NAME: &str = "50-gtk-lush-session-quiet.conf";
const HELPER_DIR_NAME: &str = "gtk-lush-session";
const SYSTEM_BUS_SOCKET: &str = "system-bus";
/// Longest socket path `dbus-daemon` will listen on: its portable
/// `_DBUS_MAX_SUN_PATH_LENGTH` (`dbus/dbus-sysdeps-unix.c`), shorter than the
/// kernel's 107 usable `sun_path` bytes. Measured on dbus 1.16: a 99-byte
/// path binds, a 100-byte one fails with "Socket name too long". The private
/// system bus socket is the session's longest, so this bounds them all.
///
/// Kind: external fact; kept exact.
const DBUS_SOCKET_PATH_MAX: usize = 99;
/// Short base for an owned runtime directory when the system temp dir would
/// make the session's socket paths too long (a nested session inside a test
/// child whose `TMPDIR` is several levels deep).
const SHORT_RUNTIME_BASE: &str = "/tmp";
/// Deadline of each readiness wait: the system bus socket, PipeWire, and
/// WirePlumber's linking policy. A healthy session is ready far sooner, so
/// reaching it means the service is broken, and the session fails.
const READY_TIMEOUT: Duration = Duration::from_secs(10);
/// Name of the last `pw-dump` a WirePlumber readiness miss saves to the
/// caller's log directory.
const NOT_READY_DUMP_FILE: &str = "pw-dump-not-ready.json";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
/// Quiet time after the command exits, so in-flight bus messages are recorded.
const DRAIN_DELAY: Duration = Duration::from_millis(300);
/// Log topic that makes a loaded module-rt visible in the daemons' logs.
const MEDIA_DEBUG: &str = "2,mod.rt:4";
/// Field and argument separators of the plan (control characters, so no
/// argument or path a lane passes can contain them).
const FIELD_SEPARATOR: char = '\u{1f}';
const ARG_SEPARATOR: char = '\u{1e}';

/// Which stage the session runs its command in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    /// The command runs inside the bus; it may start `compositor` itself.
    Session,
    /// The command runs under `mutter --headless` started by the helper.
    Run,
}

/// Options of one session. Built from a profile; explicit options override it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionOptions {
    profile: Profile,
    a11y: A11y,
    media: Media,
    portals: Portals,
    system_bus: SystemBus,
    monitors: Vec<String>,
    log_dir: Option<PathBuf>,
    inherit_runtime_dir: bool,
    expect_dirty: bool,
    recommended_environment: bool,
}

impl SessionOptions {
    /// Start from a profile's values.
    #[must_use]
    pub fn from_profile(profile: Profile) -> Self {
        Self {
            profile,
            a11y: profile.a11y,
            media: profile.media,
            portals: profile.portals,
            system_bus: profile.system_bus,
            monitors: Vec::new(),
            log_dir: None,
            inherit_runtime_dir: false,
            expect_dirty: false,
            recommended_environment: profile.name == profiles::WIDGET.name,
        }
    }

    /// Override the accessibility mode.
    #[must_use]
    pub const fn with_a11y(mut self, a11y: A11y) -> Self {
        self.a11y = a11y;
        self
    }

    /// Override the media mode.
    #[must_use]
    pub const fn with_media(mut self, media: Media) -> Self {
        self.media = media;
        self
    }

    /// Override the portal mode.
    #[must_use]
    pub const fn with_portals(mut self, portals: Portals) -> Self {
        self.portals = portals;
        self
    }

    /// Add a virtual monitor (`WxH`) for the `run` stage.
    #[must_use]
    pub fn with_monitor(mut self, monitor: impl Into<String>) -> Self {
        self.monitors.push(monitor.into());
        self
    }

    /// Keep the session's logs and hygiene report in `dir`.
    #[must_use]
    pub fn with_log_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.log_dir = Some(dir.into());
        self
    }

    /// The effective profile name.
    #[must_use]
    pub const fn profile_name(&self) -> &'static str {
        self.profile.name
    }

    fn validate(&self) -> Result<(), String> {
        if let SystemBus::Host { .. } = self.system_bus {
            if self.media == Media::Quiet {
                return Err("media are refused with the host system bus".to_string());
            }
            if self.expect_dirty {
                return Err("--expect-dirty never runs on the host system bus".to_string());
            }
        }
        if self.expect_dirty && self.media != Media::Quiet {
            return Err("--expect-dirty needs media on".to_string());
        }
        Ok(())
    }
}

/// Run `command` (program plus arguments) under `mutter --headless` in a
/// private session, and return the session's exit status.
///
/// This is the library form of `gtk-lush-session run`. A widget harness calls
/// it to relaunch itself; `extra_env` and `remove_env` apply to the whole
/// session (they are set before the bus starts).
#[must_use]
pub fn run_under_compositor(
    options: &SessionOptions,
    command: &[OsString],
    extra_env: &[(&str, &str)],
    remove_env: &[&str],
) -> ExitCode {
    let mut options = options.clone();
    if options.monitors.is_empty() {
        options
            .monitors
            .push(crate::DEFAULT_HEADLESS_MONITOR.to_string());
    }
    start_outer(&options, Stage::Run, command, extra_env, remove_env, false)
}

/// If this process is a session's inner stage, run it and return its status.
///
/// The widget harness and the `gtk-lush-session` binary call this first, so
/// the same executable serves as the outer launcher and the inner stage.
#[must_use]
pub fn run_inner_stage_if_requested() -> Option<ExitCode> {
    let plan = std::env::var(PLAN_ENV).ok()?;
    Some(match Plan::decode(&plan) {
        Ok(plan) => run_inner(&plan),
        Err(error) => {
            eprintln!("gtk-lush-session: invalid inner plan: {error}");
            ExitCode::from(TEST_FAILURE_EXIT_CODE)
        }
    })
}

// ---------------------------------------------------------------------------
// Tools

/// The session commands a set of options needs, in check order.
#[must_use]
pub fn required_commands(options: &SessionOptions, compositor: bool) -> Vec<&'static str> {
    let mut commands = vec![
        DBUS_RUN_SESSION_COMMAND,
        DBUS_DAEMON_COMMAND,
        DBUS_MONITOR_COMMAND,
        GDBUS_COMMAND,
    ];
    if compositor {
        commands.push(MUTTER_COMMAND);
    }
    if options.media == Media::Quiet {
        commands.extend([PIPEWIRE_COMMAND, WIREPLUMBER_COMMAND, PW_DUMP_COMMAND]);
    }
    commands
}

/// Resolve a command on `PATH` to an executable file.
#[must_use]
pub fn find_in_path(command: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|directory| directory.join(command))
        .find(|candidate| is_executable_file(candidate))
}

/// A path that executes this process's own binary for as long as this
/// process lives, even after the file it was started from was replaced. Only
/// for this process's own pid namespace (the inner stage it starts).
///
/// `cargo build` re-uplifts `target/debug/gtk-lush-session` (a new inode) on
/// every run, fresh or not, and `scripts/headless-session` builds before
/// each session. With several sessions starting in one worktree (`make test`
/// starts every widget shard at once), a session's `current_exe()` could name
/// a file another lane's build had just replaced, `… (deleted)`, and
/// `dbus-run-session` then failed to exec the inner stage. `/proc/<pid>/exe`
/// keeps referring to the running inode, and this process outlives the exec
/// (it waits for the session it starts).
fn own_executable() -> std::io::Result<PathBuf> {
    let link = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
    if link.exists() {
        Ok(link)
    } else {
        std::env::current_exe()
    }
}

/// The path exported as [`SESSION_BIN_ENV`]: the file this binary was started
/// from, `current` being `std::env::current_exe()`.
///
/// `current_exe()` reads `/proc/self/exe`, which Linux names
/// `<path> (deleted)` once that file was removed. `cargo build` re-uplifts
/// `target/debug/gtk-lush-session` on every run (it removes the file, then
/// links the new one), and `scripts/headless-session` builds before each
/// session, so with several lanes in one worktree the inner stage exported a
/// name no file had, and a nested `scripts/headless-session` inside the
/// session failed as an unsupported host. The suffix is dropped whenever the
/// path as named does not exist, even if the rebuilt file is not linked yet at
/// this instant: the export names the file at the original path, which is the
/// same helper, rebuilt.
fn exported_session_bin(current: PathBuf, exists: impl Fn(&Path) -> bool) -> PathBuf {
    use std::os::unix::ffi::OsStrExt as _;
    if exists(&current) {
        return current;
    }
    match current.as_os_str().as_bytes().strip_suffix(b" (deleted)") {
        Some(original) => PathBuf::from(std::ffi::OsStr::from_bytes(original)),
        None => current,
    }
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

fn missing_commands(commands: &[&'static str]) -> Vec<&'static str> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    commands
        .iter()
        .copied()
        .filter(|command| find_in_path(command, &path).is_none())
        .collect()
}

fn check_tools(options: &SessionOptions, json: bool) -> ExitCode {
    let commands = required_commands(options, true);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let missing = missing_commands(&commands);
    if json {
        let mut out = String::from("{\n");
        let _ = writeln!(out, "  \"profile\": {},", json_string(options.profile.name));
        // Which command plays which part, so a caller reads a version (the
        // compositor's, for a manifest) without naming the command itself.
        let roles = [
            ("session_bus", DBUS_RUN_SESSION_COMMAND),
            ("compositor", MUTTER_COMMAND),
            ("media", PIPEWIRE_COMMAND),
            ("session_manager", WIREPLUMBER_COMMAND),
        ];
        out.push_str("  \"roles\": {");
        out.push_str(
            &roles
                .iter()
                .map(|(role, command)| format!("{}: {}", json_string(role), json_string(command)))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("},\n");
        out.push_str("  \"commands\": {\n");
        for (index, command) in commands.iter().enumerate() {
            let resolved = find_in_path(command, &path);
            let version = resolved.as_ref().and_then(|program| version_line(program));
            let _ = write!(
                out,
                "    {}: {{\"path\": {}, \"version\": {}}}",
                json_string(command),
                resolved
                    .as_ref()
                    .map_or_else(|| "null".to_string(), |p| json_string(&p.to_string_lossy())),
                version
                    .as_deref()
                    .map_or_else(|| "null".to_string(), json_string)
            );
            out.push_str(if index + 1 == commands.len() {
                "\n"
            } else {
                ",\n"
            });
        }
        out.push_str("  },\n  \"missing\": [");
        out.push_str(
            &missing
                .iter()
                .map(|m| json_string(m))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("]\n}");
        println!("{out}");
    } else {
        for command in &missing {
            println!("{command}");
        }
    }
    if missing.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE)
    }
}

fn version_line(program: &Path) -> Option<String> {
    let output = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// The plan handed from the outer to the inner stage

#[derive(Debug, Eq, PartialEq)]
struct Plan {
    stage: Stage,
    profile: &'static str,
    a11y: A11y,
    media: Media,
    portals: Portals,
    system_bus: SystemBus,
    monitors: Vec<String>,
    log_dir: Option<PathBuf>,
    expect_dirty: bool,
    export_bin: bool,
    command: Vec<String>,
}

impl Plan {
    fn encode(&self) -> String {
        let fields = [
            "v1".to_string(),
            format!(
                "stage={}",
                if self.stage == Stage::Run {
                    "run"
                } else {
                    "session"
                }
            ),
            format!("profile={}", self.profile),
            format!("a11y={}", self.a11y),
            format!("media={}", self.media),
            format!("portals={}", self.portals),
            format!(
                "system_bus={}",
                match self.system_bus {
                    SystemBus::Private => "private".to_string(),
                    SystemBus::Host { reason } => format!("host:{reason}"),
                }
            ),
            format!("monitors={}", self.monitors.join(",")),
            format!(
                "log_dir={}",
                self.log_dir
                    .as_ref()
                    .map(|dir| dir.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
            format!("expect_dirty={}", u8::from(self.expect_dirty)),
            format!("export_bin={}", u8::from(self.export_bin)),
            format!("command={}", self.command.join(&ARG_SEPARATOR.to_string())),
        ];
        fields.join(&FIELD_SEPARATOR.to_string())
    }

    fn decode(text: &str) -> Result<Self, String> {
        let mut parts = text.split(FIELD_SEPARATOR);
        if parts.next() != Some("v1") {
            return Err("unknown plan version".to_string());
        }
        let mut plan = Self {
            stage: Stage::Session,
            profile: profiles::WIDGET.name,
            a11y: A11y::Off,
            media: Media::Off,
            portals: Portals::Off,
            system_bus: SystemBus::Private,
            monitors: Vec::new(),
            log_dir: None,
            expect_dirty: false,
            export_bin: false,
            command: Vec::new(),
        };
        for part in parts {
            let (key, value) = part.split_once('=').ok_or("malformed plan field")?;
            match key {
                "stage" => {
                    plan.stage = if value == "run" {
                        Stage::Run
                    } else {
                        Stage::Session
                    }
                }
                "profile" => {
                    plan.profile = profiles::profile_named(value)
                        .ok_or("unknown profile")?
                        .name;
                }
                "a11y" => plan.a11y = parse_a11y(value)?,
                "media" => plan.media = parse_media(value)?,
                "portals" => plan.portals = parse_portals(value)?,
                "system_bus" => {
                    plan.system_bus = if value == "private" {
                        SystemBus::Private
                    } else {
                        // Only a reason compiled into a profile can reach here;
                        // keep the text but not its identity.
                        SystemBus::Host {
                            reason: "host system bus requested",
                        }
                    };
                }
                "monitors" => {
                    plan.monitors = value
                        .split(',')
                        .filter(|m| !m.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                "log_dir" => plan.log_dir = (!value.is_empty()).then(|| PathBuf::from(value)),
                "expect_dirty" => plan.expect_dirty = value == "1",
                "export_bin" => plan.export_bin = value == "1",
                "command" => {
                    plan.command = value
                        .split(ARG_SEPARATOR)
                        .filter(|a| !a.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                _ => return Err(format!("unknown plan field {key}")),
            }
        }
        if plan.command.is_empty() {
            return Err("the plan has no command".to_string());
        }
        Ok(plan)
    }
}

fn parse_a11y(value: &str) -> Result<A11y, String> {
    match value {
        "off" => Ok(A11y::Off),
        "atspi" => Ok(A11y::Atspi),
        _ => Err(format!("--a11y must be off or atspi, not {value}")),
    }
}

fn parse_media(value: &str) -> Result<Media, String> {
    match value {
        "off" => Ok(Media::Off),
        "quiet" => Ok(Media::Quiet),
        _ => Err(format!("--media must be off or quiet, not {value}")),
    }
}

fn parse_portals(value: &str) -> Result<Portals, String> {
    match value {
        "off" => Ok(Portals::Off),
        "on" => Ok(Portals::On),
        _ => Err(format!("--portals must be off or on, not {value}")),
    }
}

// ---------------------------------------------------------------------------
// Outer stage: environment, runtime directory, `dbus-run-session`

/// Why an outer stage could not be built: the status to exit with and the
/// line to print (a `SESSION-HYGIENE:` line for a `config` violation).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionLaunchError {
    /// Exit status a launcher reports for this failure.
    pub code: u8,
    /// One line naming the failure.
    pub message: String,
}

impl SessionLaunchError {
    fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self::new(TEST_FAILURE_EXIT_CODE, message)
    }
}

impl std::fmt::Display for SessionLaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SessionLaunchError {}

/// Where an outer stage keeps its runtime directory.
enum RuntimeChoice<'a> {
    /// A fresh directory the launcher owns and releases.
    Owned,
    /// The caller's private directory (never the user's real one).
    Caller(&'a Path),
}

/// The outer stage of a session (`Stage::Session`) as a [`Command`] the
/// caller spawns and supervises itself, for a lane with its own log capture
/// and timeout (`cargo-gtk-proof`).
///
/// The command starts the session bus in `runtime_dir`, a private directory
/// the caller owns (refused when it is the user's real `XDG_RUNTIME_DIR`),
/// and re-executes the **current executable** as the inner stage, which then
/// runs `command`. The caller's `main` must therefore call
/// [`run_inner_stage_if_requested`] first. `extra_env` is set before the bus
/// starts and is visible to the profile checks (a caller's `GSK_RENDERER`,
/// `GTK_A11Y=none`). The exit status follows the `session` stage's contract,
/// [`SESSION_HYGIENE_EXIT_CODE`] included.
///
/// # Errors
///
/// A missing session command (`code` is the unsupported-host status), an
/// invalid option set or runtime directory, or a `config` violation of
/// `extra_env` (`code` is [`SESSION_HYGIENE_EXIT_CODE`]).
pub fn session_command(
    options: &SessionOptions,
    runtime_dir: &Path,
    command: &[OsString],
    extra_env: &[(&str, &str)],
) -> Result<Command, SessionLaunchError> {
    let runtime_dir = private_runtime_dir(runtime_dir).map_err(SessionLaunchError::failure)?;
    build_outer(
        options,
        Stage::Session,
        command,
        extra_env,
        &[],
        false,
        &RuntimeChoice::Caller(&runtime_dir),
    )
}

/// The compositor stage (`mutter --headless`, one virtual monitor per
/// `WxH` entry) around `command`, as a [`Command`] the caller spawns.
///
/// Valid only inside a helper session: the session exports
/// `SESSION_ACTIVE_ENV`, and the compositor must share its bus, its
/// private system bus and its quiet media.
///
/// # Errors
///
/// When called outside a helper session, or with no command.
pub fn compositor_command(
    monitors: &[String],
    command: &[OsString],
) -> Result<Command, SessionLaunchError> {
    if std::env::var_os(SESSION_ACTIVE_ENV).is_none() {
        return Err(SessionLaunchError::failure(
            "gtk-lush-session: compositor runs only inside a helper session (start one with `session`)",
        ));
    }
    if command.is_empty() {
        return Err(SessionLaunchError::failure(
            "gtk-lush-session: no command given",
        ));
    }
    let mut mutter = Command::new(MUTTER_COMMAND);
    mutter.args(["--headless", "--wayland", "--no-x11"]);
    for monitor in monitors {
        mutter.args(["--virtual-monitor", monitor]);
    }
    mutter.arg("--").args(command);
    Ok(mutter)
}

fn start_outer(
    options: &SessionOptions,
    stage: Stage,
    command: &[OsString],
    extra_env: &[(&str, &str)],
    remove_env: &[&str],
    export_bin: bool,
) -> ExitCode {
    // The runtime directory is the session's own unless the caller manages a
    // short private one (smoke lanes) and asks to keep it.
    let mut owned_runtime = None;
    let inherited;
    let runtime = if options.inherit_runtime_dir {
        match inherited_runtime_dir() {
            Ok(dir) => {
                inherited = dir;
                RuntimeChoice::Caller(&inherited)
            }
            Err(error) => {
                eprintln!("gtk-lush-session: {error}");
                return ExitCode::from(TEST_FAILURE_EXIT_CODE);
            }
        }
    } else {
        RuntimeChoice::Owned
    };
    let mut session = match build_outer(
        options, stage, command, extra_env, remove_env, export_bin, &runtime,
    ) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("{}", error.message);
            return ExitCode::from(error.code);
        }
    };
    if matches!(runtime, RuntimeChoice::Owned) {
        let prefix = format!("{}{}-", scratch::RUNTIME_DIR_PREFIX, std::process::id());
        let base = owned_runtime_base(&std::env::temp_dir(), &prefix);
        let Ok(runtime_dir) = tempfile::Builder::new().prefix(&prefix).tempdir_in(&base) else {
            eprintln!("gtk-lush-session: failed to create a private runtime directory");
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        };
        if let Some(owner) = scratch::owner_uid(runtime_dir.path()) {
            scratch::sweep_stale_scratch(
                &base,
                scratch::RUNTIME_DIR_PREFIX,
                scratch::PidlessEntries::ReclaimWhenIdle,
                owner,
            );
        }
        set_runtime_dir(&mut session, options, runtime_dir.path());
        owned_runtime = Some(runtime_dir);
    }

    let status = session.status();
    if let Some(runtime_dir) = owned_runtime {
        scratch::release_session_runtime_dir(runtime_dir);
    }
    match status {
        Ok(status) => exit_code_of(status),
        Err(error) => {
            eprintln!("gtk-lush-session: failed to start the private session bus: {error}");
            ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE)
        }
    }
}

/// Where an owned runtime directory goes: `temp_dir`, unless the private
/// system bus socket inside it (the longest socket path a session binds)
/// would not fit a unix socket address, then [`SHORT_RUNTIME_BASE`].
fn owned_runtime_base(temp_dir: &Path, prefix: &str) -> PathBuf {
    // `tempfile` appends six random characters to the prefix.
    if runtime_dir_fits_sockets(&temp_dir.join(format!("{prefix}XXXXXX"))) {
        temp_dir.to_path_buf()
    } else {
        PathBuf::from(SHORT_RUNTIME_BASE)
    }
}

/// Whether a session in runtime directory `dir` can bind its sockets: the
/// private system bus socket (`gtk-lush-session/system-bus`, longer than
/// `wayland-0` and PipeWire's `pipewire-0-manager`) must fit a unix socket
/// address. Pass the canonical path: the session canonicalizes a caller's
/// directory, and `/home` may resolve to the longer `/var/home`.
#[must_use]
pub fn runtime_dir_fits_sockets(dir: &Path) -> bool {
    dir.join(HELPER_DIR_NAME)
        .join(SYSTEM_BUS_SOCKET)
        .as_os_str()
        .len()
        <= DBUS_SOCKET_PATH_MAX
}

/// Build the bus launcher with the profile environment and the inner plan.
/// With `RuntimeChoice::Owned` the caller sets the runtime directory after
/// creating it (`set_runtime_dir`).
fn build_outer(
    options: &SessionOptions,
    stage: Stage,
    command: &[OsString],
    extra_env: &[(&str, &str)],
    remove_env: &[&str],
    export_bin: bool,
    runtime: &RuntimeChoice<'_>,
) -> Result<Command, SessionLaunchError> {
    options
        .validate()
        .map_err(|error| SessionLaunchError::failure(format!("gtk-lush-session: {error}")))?;
    let missing = missing_commands(&required_commands(options, stage == Stage::Run));
    if !missing.is_empty() {
        return Err(SessionLaunchError::new(
            UNSUPPORTED_HOST_EXIT_CODE,
            format!(
                "UNSUPPORTED-HOST: missing {}; install the session tools to run private headless GTK sessions",
                missing.join(", ")
            ),
        ));
    }
    let command = command
        .iter()
        .map(|arg| arg.to_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            SessionLaunchError::failure("gtk-lush-session: the command must be valid UTF-8")
        })?;
    if command.is_empty() {
        return Err(SessionLaunchError::failure(
            "gtk-lush-session: no command given",
        ));
    }

    let mut session = Command::new(DBUS_RUN_SESSION_COMMAND);
    for key in remove_env {
        session.env_remove(key);
    }
    for (key, value) in extra_env {
        session.env(key, value);
    }
    apply_profile_environment(&mut session, options, extra_env).map_err(|violation| {
        SessionLaunchError::new(SESSION_HYGIENE_EXIT_CODE, violation.line())
    })?;
    if let RuntimeChoice::Caller(dir) = runtime {
        set_runtime_dir(&mut session, options, dir);
    }

    let current_exe = own_executable().map_err(|_| {
        SessionLaunchError::failure("gtk-lush-session: cannot find the current executable")
    })?;
    let plan = Plan {
        stage,
        profile: options.profile.name,
        a11y: options.a11y,
        media: options.media,
        portals: options.portals,
        system_bus: options.system_bus,
        monitors: options.monitors.clone(),
        log_dir: options.log_dir.clone(),
        expect_dirty: options.expect_dirty,
        export_bin,
        command,
    };
    session
        .arg("--")
        .arg(&current_exe)
        .env(PLAN_ENV, plan.encode());
    Ok(session)
}

/// Point the session at `runtime_dir`. The private system bus's address is
/// fixed by the runtime directory, so it is exported before the session bus
/// starts: services the session bus activates later (the portal frontend is
/// a system-bus client) take their environment from it, not from the inner
/// stage.
fn set_runtime_dir(session: &mut Command, options: &SessionOptions, runtime_dir: &Path) {
    session.env("XDG_RUNTIME_DIR", runtime_dir);
    if options.system_bus == SystemBus::Private {
        session.env("DBUS_SYSTEM_BUS_ADDRESS", system_bus_address(runtime_dir));
    }
}

fn system_bus_address(runtime_dir: &Path) -> String {
    format!(
        "unix:path={}",
        runtime_dir
            .join(HELPER_DIR_NAME)
            .join(SYSTEM_BUS_SOCKET)
            .display()
    )
}

fn inherited_runtime_dir() -> Result<PathBuf, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or("--inherit-runtime-dir needs XDG_RUNTIME_DIR")?;
    private_runtime_dir(&dir)
}

/// `dir`, canonical, unless it is the user's real `XDG_RUNTIME_DIR`.
fn private_runtime_dir(dir: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .map_err(|e| e.to_string())?;
    let user_dir = PathBuf::from(format!("/run/user/{uid}"));
    let canonical = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    if canonical == user_dir || user_dir.canonicalize().is_ok_and(|user| user == canonical) {
        return Err("--inherit-runtime-dir refuses the user's real XDG_RUNTIME_DIR".to_string());
    }
    if !runtime_dir_fits_sockets(&canonical) {
        return Err(format!(
            "the runtime directory {} is too long for the session's sockets \
             ({HELPER_DIR_NAME}/{SYSTEM_BUS_SOCKET} must fit {DBUS_SOCKET_PATH_MAX} bytes); \
             use a shorter private directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

/// Apply the profile's environment to the bus launcher; `Err` is a `config`
/// violation found before anything started.
fn apply_profile_environment(
    command: &mut Command,
    options: &SessionOptions,
    extra_env: &[(&str, &str)],
) -> Result<(), Violation> {
    let caller = |key: &str| {
        extra_env
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_string())
            .or_else(|| std::env::var(key).ok())
    };
    if options.recommended_environment {
        for setting in crate::recommended_pre_gtk_environment() {
            // A caller's renderer wins, so a renderer-specific bug can be chased.
            if setting.key == "GSK_RENDERER" && caller(setting.key).is_some() {
                continue;
            }
            if setting.key == "GTK_A11Y" && caller(setting.key).is_some() {
                continue;
            }
            command.env(setting.key, setting.value);
        }
    }
    match options.a11y {
        A11y::Off => {
            let value = caller("GTK_A11Y").unwrap_or_else(|| "test".to_string());
            if !verdict::gtk_a11y_value_is_accepted(Some(&value)) {
                return Err(Violation {
                    class: ViolationClass::Config,
                    detail: format!(
                        "GTK_A11Y must be `test` or `none` with a11y off, found {value:?}"
                    ),
                });
            }
            command.env("GTK_A11Y", value).env("NO_AT_BRIDGE", "1");
        }
        A11y::Atspi => {
            command.env_remove("GTK_A11Y").env_remove("NO_AT_BRIDGE");
        }
    }
    match options.portals {
        Portals::Off => {
            command
                .env("ADW_DISABLE_PORTAL", "1")
                .env("GTK_USE_PORTAL", "0");
            if caller("GDK_DEBUG").is_none() {
                command.env("GDK_DEBUG", "no-portals");
            }
        }
        Portals::On => {
            command
                .env_remove("ADW_DISABLE_PORTAL")
                .env_remove("GTK_USE_PORTAL");
            if caller("GDK_DEBUG").as_deref() == Some("no-portals") {
                command.env_remove("GDK_DEBUG");
            }
        }
    }
    command.env(SESSION_PROFILE_ENV, options.profile.name);
    Ok(())
}

fn exit_code_of(status: ExitStatus) -> ExitCode {
    use std::os::unix::process::ExitStatusExt;
    if let Some(code) = status.code() {
        return ExitCode::from(u8::try_from(code).unwrap_or(TEST_FAILURE_EXIT_CODE));
    }
    let code = status
        .signal()
        .and_then(|signal| u8::try_from(128 + signal).ok())
        .unwrap_or(TEST_FAILURE_EXIT_CODE);
    ExitCode::from(code)
}

// ---------------------------------------------------------------------------
// Inner stage: system bus, monitors, media, the command, the guard

struct Daemon {
    name: &'static str,
    child: Child,
}

struct Monitor {
    child: Child,
    reader: JoinHandle<()>,
}

#[derive(Default)]
struct Collected {
    system_records: Vec<BusRecord>,
    names_acquired: Vec<AcquiredName>,
}

fn run_inner(plan: &Plan) -> ExitCode {
    let runtime_dir =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let helper_dir = runtime_dir.join(HELPER_DIR_NAME);
    let log_dir = plan
        .log_dir
        .clone()
        .unwrap_or_else(|| helper_dir.join("logs"));
    if let Err(error) = std::fs::create_dir_all(&log_dir) {
        eprintln!(
            "gtk-lush-session: cannot create {}: {error}",
            log_dir.display()
        );
        return ExitCode::from(TEST_FAILURE_EXIT_CODE);
    }
    let mut daemons: Vec<Daemon> = Vec::new();
    let mut monitors: Vec<Monitor> = Vec::new();
    let collected = Arc::new(Mutex::new(Collected::default()));
    let result = run_inner_body(
        plan,
        &helper_dir,
        &log_dir,
        &mut daemons,
        &mut monitors,
        &collected,
    );
    // Reverse-order teardown: media, then monitors, then the system bus.
    for daemon in daemons.iter_mut().rev() {
        stop_child(&mut daemon.child);
        let _ = daemon.name;
    }
    for monitor in monitors {
        let mut child = monitor.child;
        stop_child(&mut child);
        let _ = monitor.reader.join();
    }
    let _ = std::fs::remove_dir_all(helper_dir.join("homes"));
    if plan.log_dir.is_none() {
        let _ = std::fs::remove_dir_all(&helper_dir);
    }
    result
}

#[expect(
    clippy::too_many_lines,
    reason = "the inner stage is one ordered narrative: bus, monitors, media, command, guard"
)]
fn run_inner_body(
    plan: &Plan,
    helper_dir: &Path,
    log_dir: &Path,
    daemons: &mut Vec<Daemon>,
    monitors: &mut Vec<Monitor>,
    collected: &Arc<Mutex<Collected>>,
) -> ExitCode {
    let homes = helper_dir.join("homes");
    for directory in [helper_dir.to_path_buf(), homes.clone()] {
        if let Err(error) = std::fs::create_dir_all(&directory) {
            eprintln!(
                "gtk-lush-session: cannot create {}: {error}",
                directory.display()
            );
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
    }

    // 1. The private, monitored system bus.
    let mut system_address = None;
    if plan.system_bus == SystemBus::Private {
        let socket = helper_dir.join(SYSTEM_BUS_SOCKET);
        let address = format!("unix:path={}", socket.display());
        if std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok().as_deref() != Some(address.as_str()) {
            eprintln!(
                "gtk-lush-session: DBUS_SYSTEM_BUS_ADDRESS was not exported before the session bus"
            );
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
        let config = helper_dir.join("system-bus.conf");
        if let Err(error) = std::fs::write(&config, SYSTEM_BUS_CONFIG.replace("@LISTEN@", &address))
        {
            eprintln!("gtk-lush-session: cannot write the system bus configuration: {error}");
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
        let daemon = Command::new(DBUS_DAEMON_COMMAND)
            .arg(format!("--config-file={}", config.display()))
            .arg("--nofork")
            .arg("--nopidfile")
            .stdin(Stdio::null())
            .stdout(log_file(log_dir, "system-bus.log"))
            .stderr(log_file(log_dir, "system-bus.log"))
            .spawn();
        match daemon {
            Ok(child) => daemons.push(Daemon {
                name: "system-bus",
                child,
            }),
            Err(error) => {
                eprintln!("gtk-lush-session: cannot start the private system bus: {error}");
                return ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE);
            }
        }
        if !wait_until(|| socket.exists()) {
            eprintln!("gtk-lush-session: the private system bus did not start");
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
        match start_monitor(MonitorBus::System(&address), log_dir, collected) {
            Ok(monitor) => monitors.push(monitor),
            Err(error) => {
                eprintln!("gtk-lush-session: {error}");
                return ExitCode::from(TEST_FAILURE_EXIT_CODE);
            }
        }
        system_address = Some(address);
    }
    match start_monitor(MonitorBus::Session, log_dir, collected) {
        Ok(monitor) => monitors.push(monitor),
        Err(error) => {
            eprintln!("gtk-lush-session: {error}");
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
    }

    // 2. Quiet media.
    let mut fragments_present = true;
    let homes_private = true;
    let wireplumber_profile = if plan.expect_dirty {
        "main"
    } else {
        WIREPLUMBER_PROFILE
    };
    if plan.media == Media::Quiet {
        if let Err(error) = client_config_home_is_private() {
            eprintln!(
                "{}",
                Violation {
                    class: ViolationClass::Config,
                    detail: error
                }
                .line()
            );
            return ExitCode::from(SESSION_HYGIENE_EXIT_CODE);
        }
        if !plan.expect_dirty {
            fragments_present = write_fragments(&homes).is_ok();
        }
        let media_env = |command: &mut Command| {
            for (key, sub) in DAEMON_PRIVATE_HOMES.iter().zip(["config", "state", "data"]) {
                command.env(key, homes.join(sub));
            }
            if let Some(address) = &system_address {
                command.env("DBUS_SYSTEM_BUS_ADDRESS", address);
            }
            command
                .env("PIPEWIRE_DEBUG", MEDIA_DEBUG)
                .env("WIREPLUMBER_DEBUG", MEDIA_DEBUG)
                .stdin(Stdio::null());
        };
        for home in ["config", "state", "data"] {
            let _ = std::fs::create_dir_all(homes.join(home));
        }
        let mut pipewire = Command::new(PIPEWIRE_COMMAND);
        media_env(&mut pipewire);
        let pipewire = pipewire
            .stdout(log_file(log_dir, "pipewire.log"))
            .stderr(log_file(log_dir, "pipewire.log"))
            .spawn();
        match pipewire {
            Ok(child) => daemons.push(Daemon {
                name: PIPEWIRE_COMMAND,
                child,
            }),
            Err(error) => {
                eprintln!("gtk-lush-session: cannot start PipeWire: {error}");
                return ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE);
            }
        }
        if !wait_until(pipewire_ready) {
            eprintln!("gtk-lush-session: PipeWire did not become ready");
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
        let mut wireplumber = Command::new(WIREPLUMBER_COMMAND);
        media_env(&mut wireplumber);
        if !plan.expect_dirty {
            wireplumber.arg(format!("--profile={WIREPLUMBER_PROFILE}"));
        }
        let wireplumber = wireplumber
            .stdout(log_file(log_dir, "wireplumber.log"))
            .stderr(log_file(log_dir, "wireplumber.log"))
            .spawn();
        match wireplumber {
            Ok(child) => daemons.push(Daemon {
                name: WIREPLUMBER_COMMAND,
                child,
            }),
            Err(error) => {
                eprintln!("gtk-lush-session: cannot start WirePlumber: {error}");
                return ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE);
            }
        }
        // A capture's `pipewiresrc path=<node>` is linked by WirePlumber's
        // linking hooks, so the command starts only once they have loaded:
        // `session.services` lists `policy.linking.standard`, which
        // WirePlumber sets after every other component of its profile. A
        // miss means WirePlumber never loaded its policy; the session fails
        // before the command rather than letting a capture fail later.
        if let Err(miss) = wait_for_wireplumber_policy(READY_TIMEOUT, POLL_INTERVAL, pw_dump) {
            let path = log_dir.join(NOT_READY_DUMP_FILE);
            let dump = match (&miss.last_dump, &plan.log_dir) {
                (None, _) => MissDump::None,
                (Some(_), None) => MissDump::NoLogDir,
                (Some(text), Some(_)) => {
                    if std::fs::write(&path, text).is_ok() {
                        MissDump::Saved(&path)
                    } else {
                        MissDump::WriteFailed(&path)
                    }
                }
            };
            eprintln!("{}", wireplumber_miss_line(READY_TIMEOUT, dump));
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
    }

    // 3. The command.
    let mut command = match plan.stage {
        Stage::Run => {
            let mut command = Command::new(MUTTER_COMMAND);
            command.args(["--headless", "--wayland", "--no-x11"]);
            for monitor in &plan.monitors {
                command.args(["--virtual-monitor", monitor]);
            }
            command.arg("--").args(&plan.command);
            command
        }
        Stage::Session => {
            let mut command = Command::new(&plan.command[0]);
            command.args(&plan.command[1..]);
            command
        }
    };
    command.env_remove(PLAN_ENV).env(SESSION_ACTIVE_ENV, "1");
    if let Some(address) = &system_address {
        command.env("DBUS_SYSTEM_BUS_ADDRESS", address);
    }
    // The exported path is the file's, not `own_executable()`'s
    // `/proc/<pid>/exe`: a consumer may run in another pid namespace (the
    // GTK axiom runner passes the environment into a Flatpak SDK sandbox),
    // where that link names another process or none.
    if plan.export_bin
        && let Ok(exe) = std::env::current_exe()
    {
        command.env(SESSION_BIN_ENV, exported_session_bin(exe, Path::exists));
    }
    let status = match command.status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!(
                "gtk-lush-session: cannot start {}: {error}",
                plan.command[0]
            );
            return ExitCode::from(TEST_FAILURE_EXIT_CODE);
        }
    };

    // 4. The guard, while every service is still alive.
    std::thread::sleep(DRAIN_DELAY);
    let mut device_apis = Vec::new();
    let mut module_rt = Vec::new();
    if plan.media == Media::Quiet {
        if let Some(dump) = pw_dump() {
            device_apis = observe::device_apis(&dump);
            let _ = std::fs::write(log_dir.join("pw-dump.json"), dump);
        }
        for daemon in daemons.iter().filter(|d| d.name != "system-bus") {
            let maps = std::fs::read_to_string(format!("/proc/{}/maps", daemon.child.id()))
                .unwrap_or_default();
            module_rt.extend(observe::module_rt_evidence(daemon.name, &maps));
            let log = std::fs::read_to_string(log_dir.join(format!("{}.log", daemon.name)))
                .unwrap_or_default();
            module_rt.extend(observe::module_rt_evidence(daemon.name, &log));
        }
    }
    let gtk_a11y = std::env::var("GTK_A11Y").ok();
    let (system_bus, names_acquired) = {
        let names = session_names_now();
        let guard = collected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut acquired = guard.names_acquired.clone();
        for name in names {
            if !acquired.iter().any(|known| known.name == name) {
                acquired.push(AcquiredName {
                    name,
                    owner_exe: None,
                });
            }
        }
        (guard.system_records.clone(), acquired)
    };
    let observations = Observations {
        config: ConfigFacts {
            a11y: plan.a11y,
            media: plan.media,
            portals: plan.portals,
            gtk_a11y,
            wireplumber_profile: wireplumber_profile.to_string(),
            fragments_present,
            homes_private,
        },
        system_bus,
        device_apis,
        module_rt,
        names_acquired,
    };
    let violations = verdict::verdict(&observations);
    write_report(plan, log_dir, &observations, &violations);
    for violation in &violations {
        eprintln!("{}", violation.line());
    }

    if plan.expect_dirty {
        return expect_dirty_outcome(&violations);
    }
    if !status.success() {
        return exit_code_of(status);
    }
    if violations.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(SESSION_HYGIENE_EXIT_CODE)
    }
}

fn expect_dirty_outcome(violations: &[Violation]) -> ExitCode {
    let found: BTreeSet<_> = violations.iter().map(|v| v.class).collect();
    let required = [
        ViolationClass::Rtkit,
        ViolationClass::Bluez,
        ViolationClass::HardwareDevice,
        ViolationClass::ModuleRt,
    ];
    let missing: Vec<_> = required
        .iter()
        .filter(|class| !found.contains(class))
        .map(|c| c.name())
        .collect();
    if missing.is_empty() {
        eprintln!(
            "gtk-lush-session: --expect-dirty: the bare configuration was caught (rtkit, bluez, hardware-device, module-rt)"
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "gtk-lush-session: --expect-dirty: the guard missed {}",
            missing.join(", ")
        );
        ExitCode::from(TEST_FAILURE_EXIT_CODE)
    }
}

fn write_report(
    plan: &Plan,
    log_dir: &Path,
    observations: &Observations,
    violations: &[Violation],
) {
    let mut report = String::new();
    let _ = writeln!(report, "profile={}", plan.profile);
    let _ = writeln!(report, "a11y={}", plan.a11y);
    let _ = writeln!(report, "media={}", plan.media);
    let _ = writeln!(report, "portals={}", plan.portals);
    let _ = writeln!(report, "system_bus={}", plan.system_bus);
    if let SystemBus::Host { reason } = plan.system_bus {
        let _ = writeln!(report, "system_bus_reason={reason}");
    }
    let _ = writeln!(
        report,
        "wireplumber_profile={}",
        observations.config.wireplumber_profile
    );
    let _ = writeln!(
        report,
        "gtk_a11y={}",
        observations.config.gtk_a11y.as_deref().unwrap_or("")
    );
    let _ = writeln!(
        report,
        "system_bus_records={}",
        observations.system_bus.len()
    );
    let mut names: Vec<_> = observations
        .names_acquired
        .iter()
        .map(|n| n.name.as_str())
        .collect();
    names.sort_unstable();
    let _ = writeln!(report, "session_names={}", names.join(","));
    let _ = writeln!(
        report,
        "verdict={}",
        if violations.is_empty() {
            "clean"
        } else {
            "dirty"
        }
    );
    for violation in violations {
        let _ = writeln!(report, "{}", violation.line());
    }
    let _ = std::fs::write(log_dir.join("session-hygiene.txt"), report);
}

fn log_file(log_dir: &Path, name: &str) -> Stdio {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join(name))
        .map_or_else(|_| Stdio::null(), Stdio::from)
}

fn wait_until(mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    ready()
}

/// WirePlumber did not list its linking policy before the deadline.
#[derive(Debug, PartialEq, Eq)]
struct PolicyNotReady {
    /// The last dump that succeeded, if any.
    last_dump: Option<String>,
}

/// Polls `dump` every `poll` until a dump shows WirePlumber's linking policy
/// loaded ([`observe::wireplumber_policy_ready`]) or `deadline` has passed;
/// a passed deadline still reads once. A miss carries the last dump that
/// succeeded.
fn wait_for_wireplumber_policy(
    deadline: Duration,
    poll: Duration,
    mut dump: impl FnMut() -> Option<String>,
) -> Result<(), PolicyNotReady> {
    let deadline = Instant::now() + deadline;
    let mut last_dump = None;
    loop {
        if let Some(text) = dump() {
            if observe::wireplumber_policy_ready(&text) {
                return Ok(());
            }
            last_dump = Some(text);
        }
        if Instant::now() >= deadline {
            return Err(PolicyNotReady { last_dump });
        }
        std::thread::sleep(poll);
    }
}

/// Where a readiness miss left its last dump, for the miss line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissDump<'a> {
    /// Saved to the caller's log directory.
    Saved(&'a Path),
    /// No `pw-dump` succeeded.
    None,
    /// The log directory is the helper's own and is removed at teardown.
    NoLogDir,
    /// Writing it to the caller's log directory failed.
    WriteFailed(&'a Path),
}

/// The one bounded line a readiness miss prints. Its wording must match no
/// smoke runner's host-support skip phrase: a miss is a failure, never a skip.
fn wireplumber_miss_line(timeout: Duration, dump: MissDump<'_>) -> String {
    let location = match dump {
        MissDump::Saved(path) => format!("last pw-dump in {}", path.display()),
        MissDump::None => "no pw-dump succeeded".to_string(),
        MissDump::NoLogDir => "rerun with --log-dir to keep the last pw-dump".to_string(),
        MissDump::WriteFailed(path) => {
            format!("the last pw-dump could not be saved to {}", path.display())
        }
    };
    format!(
        "gtk-lush-session: WirePlumber did not load its linking policy within {} s (session.services never listed policy.linking.standard; {location})",
        timeout.as_secs()
    )
}

fn pipewire_ready() -> bool {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    runtime.join("pipewire-0").exists() && pw_dump().is_some()
}

fn pw_dump() -> Option<String> {
    let output = Command::new(PW_DUMP_COMMAND)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn client_config_home_is_private() -> Result<(), String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok_or(
            "media need a private XDG_CONFIG_HOME for the PipeWire client fragment; it is unset",
        )?;
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let resolve = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let config = resolve(&config);
        for tree in [".config", ".local", ".cache", ".var"] {
            if config.starts_with(resolve(&home.join(tree))) {
                return Err(format!(
                    "media need a private XDG_CONFIG_HOME; {} is inside the real home's XDG trees",
                    config.display()
                ));
            }
        }
    }
    Ok(())
}

fn write_fragments(homes: &Path) -> std::io::Result<()> {
    let config = homes.join("config");
    let targets = [
        (config.join("pipewire/pipewire.conf.d"), PIPEWIRE_FRAGMENT),
        (config.join("pipewire/client.conf.d"), PIPEWIRE_FRAGMENT),
        (
            config.join("wireplumber/wireplumber.conf.d"),
            WIREPLUMBER_FRAGMENT,
        ),
    ];
    for (dir, text) in targets {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(FRAGMENT_FILE_NAME), text)?;
    }
    // Every PipeWire client of the session (mutter's ScreenCast, pipewiresrc)
    // reads client.conf.d from the session's own config home.
    if let Some(session_config) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        let dir = session_config.join("pipewire/client.conf.d");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(FRAGMENT_FILE_NAME), PIPEWIRE_FRAGMENT)?;
    }
    Ok(())
}

fn stop_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

// ---------------------------------------------------------------------------
// Monitors

#[derive(Clone, Copy)]
enum MonitorBus<'a> {
    System(&'a str),
    Session,
}

impl MonitorBus<'_> {
    fn gdbus_args(self) -> Vec<String> {
        match self {
            Self::System(address) => vec!["--address".to_string(), address.to_string()],
            Self::Session => vec!["--session".to_string()],
        }
    }
}

fn start_monitor(
    bus: MonitorBus<'_>,
    log_dir: &Path,
    collected: &Arc<Mutex<Collected>>,
) -> Result<Monitor, String> {
    let mut command = Command::new(DBUS_MONITOR_COMMAND);
    let (log_name, is_system) = match bus {
        MonitorBus::System(address) => {
            command.args(["--address", address]);
            ("system-bus-monitor.log", true)
        }
        MonitorBus::Session => {
            command.args([
                "--session",
                "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'",
                "type='method_call',interface='org.freedesktop.DBus',member='GetId'",
            ]);
            ("session-bus-names.log", false)
        }
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log_file(log_dir, log_name))
        .spawn()
        .map_err(|error| format!("cannot start {DBUS_MONITOR_COMMAND}: {error}"))?;
    let stdout = child.stdout.take().ok_or("the monitor has no stdout")?;
    let ready = Arc::new(AtomicBool::new(false));
    let log_path = log_dir.join(log_name);
    let gdbus_args = bus.gdbus_args();
    let reader = {
        let ready = Arc::clone(&ready);
        let collected = Arc::clone(collected);
        std::thread::spawn(move || {
            read_monitor(
                stdout,
                &log_path,
                &gdbus_args,
                is_system,
                &ready,
                &collected,
            );
        })
    };
    // Readiness: the monitor has recorded our own GetId probe.
    let deadline = Instant::now() + READY_TIMEOUT;
    while !ready.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            stop_child(&mut child);
            let _ = reader.join();
            return Err("the bus monitor did not become ready".to_string());
        }
        let _ = Command::new(GDBUS_COMMAND)
            .arg("call")
            .args(bus.gdbus_args())
            .args([
                "--dest",
                "org.freedesktop.DBus",
                "--object-path",
                "/org/freedesktop/DBus",
                "--method",
                "org.freedesktop.DBus.GetId",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(POLL_INTERVAL);
    }
    Ok(Monitor { child, reader })
}

fn read_monitor(
    stdout: std::process::ChildStdout,
    log_path: &Path,
    gdbus_args: &[String],
    is_system: bool,
    ready: &AtomicBool,
    collected: &Mutex<Collected>,
) {
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .ok();
    let mut parser = observe::MonitorParser::default();
    let handle = |record: BusRecord| {
        if record.member.as_deref() == Some("GetId") {
            ready.store(true, Ordering::Release);
            return;
        }
        let mut guard = collected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if is_system {
            if record
                .args
                .iter()
                .chain(record.destination.iter())
                .chain(record.interface.iter())
                .any(|text| text.contains(verdict::RTKIT) || text.contains(verdict::BLUEZ))
            {
                let mut record = record;
                if record.sender.starts_with(':') {
                    record.sender_pid = connection_pid(gdbus_args, &record.sender);
                    record.sender_exe = record.sender_pid.and_then(executable_name);
                }
                guard.system_records.push(record);
            }
        } else if let Some((name, owner)) = observe::name_acquisition(&record)
            && !guard.names_acquired.iter().any(|known| known.name == name)
        {
            let owner_exe = connection_pid(gdbus_args, &owner).and_then(executable_name);
            guard.names_acquired.push(AcquiredName { name, owner_exe });
        }
    };
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if let Some(file) = log.as_mut() {
            let _ = writeln!(file, "{line}");
        }
        if let Some(record) = parser.push_line(&line) {
            handle(record);
        }
    }
    if let Some(record) = parser.finish() {
        handle(record);
    }
}

fn connection_pid(gdbus_args: &[String], unique_name: &str) -> Option<u32> {
    let output = Command::new(GDBUS_COMMAND)
        .arg("call")
        .args(gdbus_args)
        .args([
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.GetConnectionUnixProcessID",
            unique_name,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    observe::gdbus_uint32_reply(&String::from_utf8_lossy(&output.stdout))
}

fn executable_name(pid: u32) -> Option<String> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    exe.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

fn session_names_now() -> Vec<String> {
    Command::new(GDBUS_COMMAND)
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.ListNames",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|output| observe::gdbus_string_list(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !name.starts_with(':'))
        .collect()
}

// ---------------------------------------------------------------------------
// CLI

const USAGE: &str = "\
usage: gtk-lush-session session [OPTIONS] -- CMD...
       gtk-lush-session run [OPTIONS] [--monitor WxH]... -- CMD...
       gtk-lush-session compositor --monitor WxH [--monitor WxH]... -- CMD...
       gtk-lush-session check-tools [--profile NAME] [--media off|quiet] [--json]

Options:
  --profile widget|plain|screencast|accessibility   (default: widget)
  --a11y off|atspi  --media off|quiet  --portals off|on
  --log-dir DIR            keep the session logs and session-hygiene.txt in DIR
  --inherit-runtime-dir    use the caller's private XDG_RUNTIME_DIR
  --expect-dirty           run media with the previous bare configuration and
                           succeed only if the guard catches it (by hand)

Exit status: the command's, 77 unsupported host, 93 SESSION-HYGIENE.";

/// Entry point of the `gtk-lush-session` binary.
#[must_use]
pub fn cli_main() -> ExitCode {
    if let Some(code) = run_inner_stage_if_requested() {
        return code;
    }
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match parse_cli(&args) {
        Ok(Cli::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Cli::CheckTools { options, json }) => check_tools(&options, json),
        Ok(Cli::Compositor { monitors, command }) => compositor(&monitors, &command),
        Ok(Cli::Session {
            options,
            stage,
            command,
        }) => start_outer(&options, stage, &command, &[], &[], true),
        Err(error) => {
            eprintln!("gtk-lush-session: {error}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[derive(Debug)]
enum Cli {
    Help,
    CheckTools {
        options: SessionOptions,
        json: bool,
    },
    Compositor {
        monitors: Vec<String>,
        command: Vec<OsString>,
    },
    Session {
        options: SessionOptions,
        stage: Stage,
        command: Vec<OsString>,
    },
}

fn parse_cli(args: &[OsString]) -> Result<Cli, String> {
    let Some(subcommand) = args.first().and_then(|a| a.to_str()) else {
        return Ok(Cli::Help);
    };
    let split = args.iter().position(|arg| arg == "--");
    let (flags, command) = match split {
        Some(index) => (&args[1..index], args[index + 1..].to_vec()),
        None => (&args[1..], Vec::new()),
    };
    let flags: Vec<&str> = flags
        .iter()
        .map(|f| f.to_str().ok_or("non-UTF-8 option"))
        .collect::<Result<_, _>>()?;
    let mut profile = profiles::WIDGET;
    let mut overrides: Vec<(&str, &str)> = Vec::new();
    let mut monitors = Vec::new();
    let mut log_dir = None;
    let mut json = false;
    let mut inherit = false;
    let mut expect_dirty = false;
    let mut index = 0;
    while index < flags.len() {
        let flag = flags[index];
        let value = || {
            flags
                .get(index + 1)
                .copied()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag {
            "-h" | "--help" => return Ok(Cli::Help),
            "--json" => json = true,
            "--inherit-runtime-dir" => inherit = true,
            "--expect-dirty" => expect_dirty = true,
            "--profile" => {
                profile = profiles::profile_named(value()?)
                    .ok_or_else(|| format!("unknown profile {}", flags[index + 1]))?;
                index += 1;
            }
            "--a11y" | "--media" | "--portals" => {
                overrides.push((flag, value()?));
                index += 1;
            }
            "--monitor" => {
                monitors.push(value()?.to_string());
                index += 1;
            }
            "--log-dir" => {
                log_dir = Some(PathBuf::from(value()?));
                index += 1;
            }
            other => return Err(format!("unknown option {other}")),
        }
        index += 1;
    }
    let mut options = SessionOptions::from_profile(profile);
    for (flag, value) in overrides {
        options = match flag {
            "--a11y" => options.with_a11y(parse_a11y(value)?),
            "--media" => options.with_media(parse_media(value)?),
            _ => options.with_portals(parse_portals(value)?),
        };
    }
    options.monitors = monitors;
    options.log_dir = log_dir;
    options.inherit_runtime_dir = inherit;
    options.expect_dirty = expect_dirty;
    match subcommand {
        "-h" | "--help" | "help" => Ok(Cli::Help),
        "check-tools" => Ok(Cli::CheckTools { options, json }),
        "compositor" => {
            if options.monitors.is_empty() {
                return Err("compositor needs --monitor WxH".to_string());
            }
            if command.is_empty() {
                return Err("compositor needs -- CMD".to_string());
            }
            Ok(Cli::Compositor {
                monitors: options.monitors,
                command,
            })
        }
        "session" | "run" => {
            if command.is_empty() {
                return Err(format!("{subcommand} needs -- CMD"));
            }
            let stage = if subcommand == "run" {
                Stage::Run
            } else {
                Stage::Session
            };
            if stage == Stage::Run && options.monitors.is_empty() {
                options
                    .monitors
                    .push(crate::DEFAULT_HEADLESS_MONITOR.to_string());
            }
            Ok(Cli::Session {
                options,
                stage,
                command,
            })
        }
        other => Err(format!("unknown subcommand {other}")),
    }
}

/// Exec `mutter --headless` for `command`, so the caller's handle is mutter.
fn compositor(monitors: &[String], command: &[OsString]) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let mut mutter = match compositor_command(monitors, command) {
        Ok(mutter) => mutter,
        Err(error) => {
            eprintln!("{}", error.message);
            return ExitCode::from(error.code);
        }
    };
    let error = mutter.exec();
    eprintln!("gtk-lush-session: cannot exec the compositor: {error}");
    ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_own_executable_survives_a_replaced_binary() {
        let own = own_executable().expect("own executable");
        assert!(
            own.starts_with("/proc"),
            "Linux runs the helper, so the path is the process's own exe link: {}",
            own.display()
        );
        assert_eq!(
            std::fs::canonicalize(&own).expect("resolve the link"),
            std::env::current_exe().expect("current exe"),
        );
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn an_owned_runtime_dir_keeps_its_sockets_bindable() {
        let prefix = "gtk-lush-proof-runtime-4242-";
        assert_eq!(
            owned_runtime_base(Path::new("/tmp/scope"), prefix),
            Path::new("/tmp/scope")
        );
        let deep = Path::new("/tmp")
            .join("lushtext-temp-scope-abcdef/lushtext-test-4242-abcdef/child-17-attempt-1");
        assert_eq!(
            owned_runtime_base(&deep, prefix),
            Path::new(SHORT_RUNTIME_BASE)
        );
        assert!(runtime_dir_fits_sockets(
            &Path::new(SHORT_RUNTIME_BASE).join(format!("{prefix}XXXXXX"))
        ));
        // A nested session under `with-temp-scope.sh`'s TMPDIR: its system
        // bus socket (101 bytes) fits the kernel's 107 but not dbus's 99.
        let scope = Path::new("/tmp/lt-scope-3891546-axioms-DAEBkW");
        let prefix = "gtk-lush-proof-runtime-3891773-";
        assert_eq!(
            owned_runtime_base(scope, prefix),
            Path::new(SHORT_RUNTIME_BASE)
        );
        assert!(runtime_dir_fits_sockets(
            &Path::new(SHORT_RUNTIME_BASE).join(format!("{prefix}XXXXXX"))
        ));
        assert!(runtime_dir_fits_sockets(Path::new(&"r".repeat(
            DBUS_SOCKET_PATH_MAX - format!("/{HELPER_DIR_NAME}/{SYSTEM_BUS_SOCKET}").len()
        ))));
        assert!(!runtime_dir_fits_sockets(Path::new(&"r".repeat(
            DBUS_SOCKET_PATH_MAX + 1 - format!("/{HELPER_DIR_NAME}/{SYSTEM_BUS_SOCKET}").len()
        ))));
    }

    #[test]
    fn a_caller_runtime_dir_too_long_for_the_sockets_is_refused() {
        // Under the short base, so `rt` fits whatever TMPDIR the test runs in
        // (an agent stage's is too deep on its own).
        let root = tempfile::Builder::new()
            .prefix("lt-harness-test-")
            .tempdir_in(SHORT_RUNTIME_BASE)
            .expect("tempdir");
        let deep = root.path().join("x".repeat(120));
        std::fs::create_dir(&deep).expect("deep dir");
        let error = private_runtime_dir(&deep).expect_err("too long for a socket");
        assert!(
            error.contains("too long for the session's sockets"),
            "{error}"
        );
        let short = root.path().join("rt");
        std::fs::create_dir(&short).expect("short dir");
        assert!(private_runtime_dir(&short).is_ok());
    }

    #[test]
    fn plan_round_trips() {
        let plan = Plan {
            stage: Stage::Run,
            profile: "screencast",
            a11y: A11y::Off,
            media: Media::Quiet,
            portals: Portals::On,
            system_bus: SystemBus::Private,
            monitors: vec!["1280x720".into(), "640x480".into()],
            log_dir: Some(PathBuf::from("/tmp/x y")),
            expect_dirty: false,
            export_bin: true,
            command: vec!["python3".into(), "a b".into(), "--x=1".into()],
        };
        assert_eq!(Plan::decode(&plan.encode()), Ok(plan));
        assert!(Plan::decode("v0").is_err());
    }

    #[test]
    fn cli_profiles_and_overrides() {
        let Ok(Cli::Session {
            options,
            stage,
            command,
        }) = parse_cli(&os(&[
            "run",
            "--profile",
            "screencast",
            "--media",
            "off",
            "--",
            "app",
            "--flag",
        ]))
        else {
            panic!("run parses");
        };
        assert_eq!(stage, Stage::Run);
        assert_eq!(options.profile_name(), "screencast");
        assert_eq!(options.media, Media::Off);
        assert_eq!(options.portals, Portals::On);
        assert_eq!(options.monitors, [crate::DEFAULT_HEADLESS_MONITOR]);
        assert_eq!(command, os(&["app", "--flag"]));
        assert!(parse_cli(&os(&["session", "--profile", "main", "--", "x"])).is_err());
        assert!(parse_cli(&os(&["compositor", "--", "x"])).is_err());
        assert!(parse_cli(&os(&["session"])).is_err());
        assert!(matches!(
            parse_cli(&os(&["check-tools", "--json"])),
            Ok(Cli::CheckTools { json: true, .. })
        ));
    }

    #[test]
    fn host_bus_refuses_media_and_expect_dirty_refuses_the_host() {
        let host = Profile {
            system_bus: SystemBus::Host { reason: "test" },
            media: Media::Off,
            ..profiles::PLAIN
        };
        assert!(SessionOptions::from_profile(host).validate().is_ok());
        assert!(
            SessionOptions::from_profile(host)
                .with_media(Media::Quiet)
                .validate()
                .is_err()
        );
        let mut dirty = SessionOptions::from_profile(profiles::SCREENCAST);
        dirty.expect_dirty = true;
        assert!(dirty.validate().is_ok());
        let mut dirty_host = SessionOptions::from_profile(host);
        dirty_host.expect_dirty = true;
        assert!(dirty_host.validate().is_err());
    }

    #[test]
    fn profile_environment_is_set_before_the_bus() {
        let mut command = Command::new("true");
        apply_profile_environment(
            &mut command,
            &SessionOptions::from_profile(profiles::WIDGET),
            &[("GSK_RENDERER", "ngl")],
        )
        .expect("clean");
        let envs: Vec<_> = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let get = |key: &str| {
            envs.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.clone())
        };
        assert_eq!(get("GTK_A11Y").as_deref(), Some("test"));
        assert_eq!(get("NO_AT_BRIDGE").as_deref(), Some("1"));
        assert_eq!(get("ADW_DISABLE_PORTAL").as_deref(), Some("1"));
        assert_eq!(
            get("GTK_IM_MODULE").as_deref(),
            Some("gtk-im-context-simple")
        );
        // The caller's renderer wins.
        assert_eq!(get("GSK_RENDERER"), None);

        let mut command = Command::new("true");
        assert!(
            apply_profile_environment(
                &mut command,
                &SessionOptions::from_profile(profiles::PLAIN),
                &[("GTK_A11Y", "none")]
            )
            .is_ok()
        );
        let mut command = Command::new("true");
        let refused = apply_profile_environment(
            &mut command,
            &SessionOptions::from_profile(profiles::PLAIN),
            &[("GTK_A11Y", "atspi")],
        );
        assert_eq!(refused.map_err(|v| v.class), Err(ViolationClass::Config));

        let mut command = Command::new("true");
        apply_profile_environment(
            &mut command,
            &SessionOptions::from_profile(profiles::ACCESSIBILITY),
            &[],
        )
        .expect("clean");
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(removed.iter().any(|k| k == "GTK_A11Y"));
        assert!(removed.iter().any(|k| k == "ADW_DISABLE_PORTAL"));
    }

    #[test]
    fn required_commands_follow_the_options() {
        let widget = required_commands(&SessionOptions::from_profile(profiles::WIDGET), true);
        assert!(widget.contains(&MUTTER_COMMAND));
        assert!(!widget.contains(&PIPEWIRE_COMMAND));
        let screencast =
            required_commands(&SessionOptions::from_profile(profiles::SCREENCAST), false);
        assert!(screencast.contains(&WIREPLUMBER_COMMAND));
        assert!(!screencast.contains(&MUTTER_COMMAND));
    }

    #[test]
    fn executable_lookup_needs_the_execute_bit() {
        use std::os::unix::fs::PermissionsExt;
        let tempdir = tempfile::tempdir().expect("tempdir");
        let tool = tempdir.path().join(DBUS_RUN_SESSION_COMMAND);
        std::fs::write(&tool, "").expect("fixture");
        assert!(find_in_path(DBUS_RUN_SESSION_COMMAND, tempdir.path().as_os_str()).is_none());
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            find_in_path(DBUS_RUN_SESSION_COMMAND, tempdir.path().as_os_str()),
            Some(tool)
        );
        assert!(find_in_path(MUTTER_COMMAND, tempdir.path().as_os_str()).is_none());
    }

    #[test]
    fn the_wireplumber_wait_starts_the_command_at_the_first_ready_dump() {
        let ready = "\"session.services\": \"[policy.default-nodes, policy.linking.standard]\"";
        let mut calls = 0;
        let outcome =
            wait_for_wireplumber_policy(Duration::from_secs(5), Duration::from_millis(1), || {
                calls += 1;
                match calls {
                    1 => None,
                    2 => Some("[]".to_string()),
                    _ => Some(ready.to_string()),
                }
            });
        assert_eq!(outcome, Ok(()));
        assert_eq!(calls, 3, "ready on the third call, and no call after it");
    }

    #[test]
    fn the_wireplumber_wait_misses_at_its_deadline_with_the_last_dump() {
        let deadline = Duration::from_millis(20);
        let started = Instant::now();
        let mut calls = 0u32;
        let outcome = wait_for_wireplumber_policy(deadline, Duration::from_millis(1), || {
            calls += 1;
            // A failed dump after a successful one keeps the successful one.
            (!calls.is_multiple_of(3)).then(|| format!("dump {calls}"))
        });
        let elapsed = started.elapsed();
        let Err(miss) = outcome else {
            panic!("a source that is never ready misses");
        };
        let last = miss.last_dump.expect("a dump succeeded");
        assert!(last.starts_with("dump "), "{last}");
        let index: u32 = last["dump ".len()..].parse().expect("dump index");
        assert!(index + 1 >= calls && index <= calls, "{index} of {calls}");
        assert!(
            elapsed >= deadline,
            "it polled until the deadline: {elapsed:?}"
        );
        assert!(
            elapsed < deadline + Duration::from_secs(5),
            "it stopped at the deadline: {elapsed:?}"
        );
        let mut after = 0;
        let never = wait_for_wireplumber_policy(Duration::ZERO, Duration::from_millis(1), || {
            after += 1;
            None
        });
        assert_eq!(never, Err(PolicyNotReady { last_dump: None }));
        assert_eq!(after, 1, "a passed deadline still reads once");
    }

    #[test]
    fn a_wireplumber_miss_prints_one_line_saying_where_the_dump_is() {
        let path = Path::new("/run/logs/pw-dump-not-ready.json");
        let prefix = "gtk-lush-session: WirePlumber did not load its linking policy within 10 s (session.services never listed policy.linking.standard; ";
        let timeout = Duration::from_secs(10);
        for (dump, location) in [
            (
                MissDump::Saved(path),
                "last pw-dump in /run/logs/pw-dump-not-ready.json",
            ),
            (MissDump::None, "no pw-dump succeeded"),
            (
                MissDump::NoLogDir,
                "rerun with --log-dir to keep the last pw-dump",
            ),
            (
                MissDump::WriteFailed(path),
                "the last pw-dump could not be saved to /run/logs/pw-dump-not-ready.json",
            ),
        ] {
            assert_eq!(
                wireplumber_miss_line(timeout, dump),
                format!("{prefix}{location})")
            );
        }
    }

    #[test]
    fn the_exported_session_bin_names_the_file_after_a_rebuild_replaced_it() {
        use std::os::unix::ffi::OsStrExt as _;
        let rebuilt = Path::new("/w/target/debug/gtk-lush-session");
        let replaced = PathBuf::from("/w/target/debug/gtk-lush-session (deleted)");
        // `/proc/self/exe` after a rebuild: the suffix goes, whether or not
        // the new file is in place at this instant (cargo removes, then links).
        assert_eq!(exported_session_bin(replaced.clone(), |_| false), rebuilt);
        assert_eq!(
            exported_session_bin(replaced, |path| path == rebuilt),
            rebuilt
        );
        // A path that exists as named is kept, even with that suffix.
        let named = PathBuf::from("/w/odd (deleted)");
        assert_eq!(exported_session_bin(named.clone(), |_| true), named);
        // A missing path without the suffix is kept.
        let plain = PathBuf::from("/w/gone");
        assert_eq!(exported_session_bin(plain.clone(), |_| false), plain);
        // Non-UTF-8 names are stripped by bytes.
        let raw = PathBuf::from(std::ffi::OsStr::from_bytes(b"/w/\xff-bin (deleted)"));
        assert_eq!(
            exported_session_bin(raw, |_| false),
            PathBuf::from(std::ffi::OsStr::from_bytes(b"/w/\xff-bin"))
        );
    }

    #[test]
    fn json_strings_escape() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(SESSION_HYGIENE_EXIT_CODE, 93);
        assert_ne!(SESSION_HYGIENE_EXIT_CODE, UNSUPPORTED_HOST_EXIT_CODE);
        assert_ne!(SESSION_HYGIENE_EXIT_CODE, TEST_FAILURE_EXIT_CODE);
    }
}
