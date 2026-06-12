// SPDX-License-Identifier: MIT OR Apache-2.0

//! Headless GTK proof harness helpers for gtk-rs applications.
//!
//! This crate owns the reusable mechanics of a widget-test harness:
//! private headless Mutter launch, per-test child process isolation,
//! filter/list handling, bounded retry reporting, and GLib main-loop wait
//! helpers. Consumer applications keep their own GTK initialization, resource
//! registration, fixture setup, and test registry generation.
//!
//! GTK Lush crates remain independently adoptable leaf crates. They do not own
//! GTK control flow, define a view DSL, add a state/message framework, depend
//! on another GTK Lush crate, or replace Libadwaita adaptive behavior.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use glib::prelude::IsA;
use gtk4::prelude::GtkWindowExt;

/// Exit code used when the harness itself or a child test fails.
///
/// Rust's default test harness exits with `101` for test failure. Reusing that
/// value keeps custom harness failures familiar to cargo, CI, and developers.
pub const TEST_FAILURE_EXIT_CODE: u8 = 101;

/// Exit code used when host compositor/session tooling is unavailable.
///
/// The code follows the common test-runner convention for environment skips,
/// which lets CI and scripts distinguish an unsupported desktop host from an
/// application test failure.
pub const UNSUPPORTED_HOST_EXIT_CODE: u8 = 77;

/// Default virtual monitor geometry for headless widget-test sessions.
///
/// The size is intentionally larger than common laptop windows so wide
/// sidebars, dialogs, and editor chrome can realize without accidentally
/// entering compact layout while ordinary tests run.
pub const DEFAULT_HEADLESS_MONITOR: &str = "2560x1600";

/// Default attempts per selected test.
///
/// The first failure is retried in a fresh child process to identify one-off
/// compositor timing transients while still reporting every retry pass as
/// `FLAKY`.
pub const DEFAULT_TEST_ATTEMPTS: usize = 2;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// A single test entry known to the custom widget harness.
#[derive(Clone, Copy)]
pub struct RegisteredTest {
    name: &'static str,
    test_fn: fn(),
}

impl RegisteredTest {
    /// Create a test entry from its stable display name and function pointer.
    #[must_use]
    pub const fn new(name: &'static str, test_fn: fn()) -> Self {
        Self { name, test_fn }
    }

    /// Return the stable test display name used for filtering and output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    fn run(self) {
        (self.test_fn)();
    }
}

impl fmt::Debug for RegisteredTest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisteredTest")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Environment contract used to detect parent and child harness phases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessConfig {
    child_test_env: &'static str,
    headless_runner_env: &'static str,
    headless_monitor_env: &'static str,
    default_headless_monitor: &'static str,
    test_attempts: usize,
    runner_label: &'static str,
}

impl HarnessConfig {
    /// Create a harness configuration from caller-owned environment names.
    ///
    /// The environment names stay caller-owned so applications can preserve
    /// existing script and CI contracts while sharing the generic harness.
    #[must_use]
    pub const fn new(
        child_test_env: &'static str,
        headless_runner_env: &'static str,
        headless_monitor_env: &'static str,
    ) -> Self {
        Self {
            child_test_env,
            headless_runner_env,
            headless_monitor_env,
            default_headless_monitor: DEFAULT_HEADLESS_MONITOR,
            test_attempts: DEFAULT_TEST_ATTEMPTS,
            runner_label: "widget tests",
        }
    }

    /// Set the default virtual monitor geometry used by the headless session.
    #[must_use]
    pub const fn with_default_headless_monitor(mut self, monitor: &'static str) -> Self {
        self.default_headless_monitor = monitor;
        self
    }

    /// Set the number of child-process attempts per selected test.
    ///
    /// A value of zero is treated as one attempt so a misconfigured caller does
    /// not silently skip every selected test.
    #[must_use]
    pub const fn with_test_attempts(mut self, attempts: usize) -> Self {
        self.test_attempts = attempts;
        self
    }

    /// Set the human-readable label printed when relaunching headlessly.
    #[must_use]
    pub const fn with_runner_label(mut self, label: &'static str) -> Self {
        self.runner_label = label;
        self
    }

    /// Return the environment variable that names the child test to run.
    #[must_use]
    pub const fn child_test_env(&self) -> &'static str {
        self.child_test_env
    }

    /// Return the environment variable that marks a process as headless-owned.
    #[must_use]
    pub const fn headless_runner_env(&self) -> &'static str {
        self.headless_runner_env
    }

    /// Return the environment variable that overrides virtual monitor geometry.
    #[must_use]
    pub const fn headless_monitor_env(&self) -> &'static str {
        self.headless_monitor_env
    }

    fn attempts(&self) -> usize {
        self.test_attempts.max(1)
    }
}

/// One recommended process environment setting for GTK widget tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecommendedEnvironment {
    /// Environment variable name.
    pub key: &'static str,
    /// Environment variable value.
    pub value: &'static str,
}

/// Return environment settings that callers should apply before GTK starts.
///
/// The crate returns data instead of mutating the current process so it can
/// remain `unsafe`-free. In Rust 2024, changing process environment is unsafe
/// once other threads may exist, and only the application knows that startup
/// invariant.
#[must_use]
pub const fn recommended_pre_gtk_environment() -> [RecommendedEnvironment; 4] {
    [
        RecommendedEnvironment {
            key: "NO_AT_BRIDGE",
            value: "1",
        },
        RecommendedEnvironment {
            key: "GDK_DEBUG",
            value: "no-portals",
        },
        RecommendedEnvironment {
            key: "GTK_USE_PORTAL",
            value: "0",
        },
        RecommendedEnvironment {
            key: "GSK_RENDERER",
            value: "cairo",
        },
    ]
}

/// Apply safe child-process environment defaults to a command.
pub fn apply_headless_child_environment(command: &mut Command, config: &HarnessConfig) {
    command
        .env("GDK_BACKEND", "wayland")
        .env(config.headless_runner_env(), "1")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY");
}

/// Run a custom GTK widget-test harness over registered tests.
///
/// The parent process relaunches into a private headless compositor unless the
/// configured runner environment variable is already present. Inside the
/// headless session, the parent process lists or filters tests, then spawns a
/// fresh child process for each selected test.
#[must_use]
pub fn run_registered_tests(
    tests: &[RegisteredTest],
    config: &HarnessConfig,
    args: &[String],
) -> ExitCode {
    let selection = TestSelection::from_args(args);
    if !selection.list_mode && std::env::var_os(config.headless_runner_env()).is_none() {
        return run_under_headless_compositor(config, args);
    }

    if let Ok(test_name) = std::env::var(config.child_test_env()) {
        return run_single_test(tests, &test_name);
    }

    let selected = select_tests(tests, &selection);
    if selection.list_mode {
        return list_tests(&selected, selection.terse_list);
    }

    run_selected_tests(&selected, config, args)
}

fn run_under_headless_compositor(config: &HarnessConfig, args: &[String]) -> ExitCode {
    if let Err(missing) = check_headless_tooling() {
        eprintln!(
            "UNSUPPORTED-HOST: missing {missing}; install dbus-run-session and mutter \
             to run private headless GTK tests"
        );
        return ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE);
    }

    let Ok(runtime_dir) = tempfile::Builder::new()
        .prefix("gtk-lush-proof-runtime-")
        .tempdir()
    else {
        eprintln!("failed to create private widget-test runtime directory");
        return ExitCode::from(TEST_FAILURE_EXIT_CODE);
    };
    let monitor = std::env::var(config.headless_monitor_env())
        .unwrap_or_else(|_| config.default_headless_monitor.to_string());
    let Ok(current_exe) = std::env::current_exe() else {
        eprintln!("failed to find current test executable for headless relaunch");
        return ExitCode::from(TEST_FAILURE_EXIT_CODE);
    };

    eprintln!(
        "running {} under private mutter --headless session ({monitor}); \
         live desktop display is not used",
        config.runner_label
    );

    let mut command = Command::new("dbus-run-session");
    command
        .arg("--")
        .arg("mutter")
        .arg("--headless")
        .arg("--wayland")
        .arg("--no-x11")
        .arg("--virtual-monitor")
        .arg(&monitor)
        .arg("--")
        .arg(current_exe)
        .args(args)
        .env("XDG_RUNTIME_DIR", runtime_dir.path());
    apply_headless_child_environment(&mut command, config);

    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => status_to_exit_code(status.code()),
        Err(error) => {
            eprintln!(
                "failed to launch private headless test session: {error}; \
                 install dbus-run-session and mutter"
            );
            ExitCode::from(UNSUPPORTED_HOST_EXIT_CODE)
        }
    }
}

fn check_headless_tooling() -> Result<(), &'static str> {
    let Some(path) = std::env::var_os("PATH") else {
        return Err("PATH");
    };
    if !command_exists_in_path("dbus-run-session", &path) {
        return Err("dbus-run-session");
    }
    if !command_exists_in_path("mutter", &path) {
        return Err("mutter");
    }
    Ok(())
}

fn command_exists_in_path(command: &str, path: &std::ffi::OsStr) -> bool {
    std::env::split_paths(path).any(|directory| {
        let candidate = directory.join(command);
        is_executable_file(&candidate)
    })
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn status_to_exit_code(code: Option<i32>) -> ExitCode {
    ExitCode::from(
        code.and_then(|value| u8::try_from(value).ok())
            .unwrap_or(TEST_FAILURE_EXIT_CODE),
    )
}

fn list_tests(selected: &[RegisteredTest], terse_list: bool) -> ExitCode {
    for test in selected {
        println!("{}: test", test.name());
    }
    if !terse_list {
        println!("\n{} tests, 0 benchmarks", selected.len());
    }
    ExitCode::SUCCESS
}

fn run_selected_tests(
    selected: &[RegisteredTest],
    config: &HarnessConfig,
    args: &[String],
) -> ExitCode {
    println!("running {} tests", selected.len());

    let Ok(current_exe) = std::env::current_exe() else {
        eprintln!("failed to find current test executable for child test run");
        return ExitCode::from(TEST_FAILURE_EXIT_CODE);
    };
    let mut failed = Vec::new();
    let mut flaky = Vec::new();

    for test in selected {
        print!("test {} ... ", test.name());
        let _ = io::stdout().flush();

        let mut passed_on = None;
        for attempt in 1..=config.attempts() {
            let status = Command::new(&current_exe)
                .env(config.child_test_env(), test.name())
                .env(config.headless_runner_env(), "1")
                .args(args)
                .status();
            if status.is_ok_and(|status| status.success()) {
                passed_on = Some(attempt);
                break;
            }
        }

        match passed_on {
            Some(1) => println!("ok"),
            Some(attempt) => {
                println!("ok (FLAKY: passed on attempt {attempt})");
                flaky.push(test.name());
            }
            None => {
                println!("FAILED");
                failed.push(test.name());
            }
        }
    }

    report_flakes(&flaky);
    report_result(&failed, flaky.len())
}

fn report_flakes(flaky: &[&'static str]) {
    if flaky.is_empty() {
        return;
    }

    eprintln!(
        "\nFLAKY SUMMARY: {} widget test(s) passed only on retry - investigate and fix the root cause:",
        flaky.len()
    );
    for name in flaky {
        eprintln!("    FLAKY: {name}");
    }
}

fn report_result(failed: &[&'static str], flaky_count: usize) -> ExitCode {
    if failed.is_empty() {
        if flaky_count == 0 {
            println!("\ntest result: ok. all tests passed");
        } else {
            println!("\ntest result: ok. all tests passed ({flaky_count} flaky on retry)");
        }
        return ExitCode::SUCCESS;
    }

    println!("\nfailures:");
    for name in failed {
        println!("    {name}");
    }
    println!("\ntest result: FAILED. {} failed", failed.len());
    ExitCode::from(TEST_FAILURE_EXIT_CODE)
}

fn run_single_test(tests: &[RegisteredTest], name: &str) -> ExitCode {
    let Some(test) = tests.iter().copied().find(|test| test.name() == name) else {
        eprintln!("unknown widget test: {name}");
        return ExitCode::from(TEST_FAILURE_EXIT_CODE);
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| test.run()));
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(TEST_FAILURE_EXIT_CODE)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TestSelection {
    list_mode: bool,
    terse_list: bool,
    exact: bool,
    skip_filters: Vec<String>,
    name_filters: Vec<String>,
}

impl TestSelection {
    fn from_args(args: &[String]) -> Self {
        let list_mode = args.iter().any(|arg| arg == "--list");
        let terse_list = args
            .array_windows::<2>()
            .any(|[flag, value]| flag == "--format" && value == "terse");
        let exact = args.iter().any(|arg| arg == "--exact");
        let skip_filters: Vec<String> = args
            .array_windows::<2>()
            .filter(|&[flag, _value]| flag == "--skip")
            .map(|[_flag, value]| value.clone())
            .collect();
        let name_filters: Vec<String> = args
            .iter()
            .filter(|arg| {
                !arg.starts_with('-')
                    && *arg != "terse"
                    && *arg != "pretty"
                    && !skip_filters.contains(arg)
            })
            .cloned()
            .collect();

        Self {
            list_mode,
            terse_list,
            exact,
            skip_filters,
            name_filters,
        }
    }
}

fn select_tests(tests: &[RegisteredTest], selection: &TestSelection) -> Vec<RegisteredTest> {
    tests
        .iter()
        .copied()
        .filter(|test| matches_filters(test.name(), selection))
        .collect()
}

fn matches_filters(name: &str, selection: &TestSelection) -> bool {
    let included = if selection.name_filters.is_empty() {
        true
    } else if selection.exact {
        selection.name_filters.iter().any(|filter| name == filter)
    } else {
        selection
            .name_filters
            .iter()
            .any(|filter| name.contains(filter))
    };

    included
        && !selection
            .skip_filters
            .iter()
            .any(|skip| name.contains(skip))
}

/// Run the GTK main loop until no immediate events remain.
pub fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

/// Sleep briefly, then drain pending GTK main-loop work.
pub fn flush_after_delay(delay: Duration) {
    std::thread::sleep(delay);
    flush_events();
}

/// Poll until a predicate becomes true or the timeout expires.
///
/// Each poll sleeps briefly and then drains all ready main-loop sources. That
/// drain-to-exhaustion behavior is load-bearing for helpers such as
/// `gtk-lush-tasks`, whose worker completions arrive through low-priority GLib
/// idle callbacks.
///
/// # Panics
///
/// Panics when the predicate does not become true before `timeout`.
pub fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(DEFAULT_POLL_INTERVAL);
        flush_events();
    }
    panic!("condition was not met within {timeout:?}");
}

/// Present a GTK window and flush the initial realization work.
pub fn present_window(window: &impl IsA<gtk4::Window>) {
    window.present();
    flush_events();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn passing_test() {}

    #[test]
    fn selection_supports_list_exact_and_skip() {
        let args = vec![
            "--list".to_string(),
            "--format".to_string(),
            "terse".to_string(),
            "--exact".to_string(),
            "--skip".to_string(),
            "slow".to_string(),
            "module::fast".to_string(),
        ];

        let selection = TestSelection::from_args(&args);
        assert!(selection.list_mode);
        assert!(selection.terse_list);
        assert!(selection.exact);
        assert_eq!(selection.skip_filters, ["slow"]);
        assert_eq!(selection.name_filters, ["module::fast"]);
    }

    #[test]
    fn selected_tests_filter_by_substring_and_skip() {
        let tests = [
            RegisteredTest::new("module::fast", passing_test),
            RegisteredTest::new("module::slow", passing_test),
            RegisteredTest::new("other::fast", passing_test),
        ];
        let selection = TestSelection {
            list_mode: false,
            terse_list: false,
            exact: false,
            skip_filters: vec!["slow".to_string()],
            name_filters: vec!["module".to_string()],
        };

        let selected = select_tests(&tests, &selection);
        let names: Vec<_> = selected.iter().map(|test| test.name()).collect();
        assert_eq!(names, ["module::fast"]);
    }

    #[test]
    fn recommended_environment_is_safe_to_apply_before_gtk() {
        let values = recommended_pre_gtk_environment();
        assert_eq!(values[0].key, "NO_AT_BRIDGE");
        assert_eq!(values[1].value, "no-portals");
        assert_eq!(values[2].key, "GTK_USE_PORTAL");
        assert_eq!(values[3].value, "cairo");
    }

    #[test]
    fn headless_tooling_probe_checks_required_commands() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let tempdir = tempfile::tempdir().expect("tempdir");
        let dbus = tempdir.path().join("dbus-run-session");
        let mutter = tempdir.path().join("mutter");
        std::fs::write(&dbus, "").expect("dbus fixture");

        assert!(!command_exists_in_path(
            "dbus-run-session",
            tempdir.path().as_os_str()
        ));
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&dbus)
                .expect("dbus metadata")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&dbus, permissions).expect("dbus permissions");
        }
        assert!(command_exists_in_path(
            "dbus-run-session",
            tempdir.path().as_os_str()
        ));
        assert!(!command_exists_in_path(
            "mutter",
            tempdir.path().as_os_str()
        ));

        std::fs::write(&mutter, "").expect("mutter fixture");
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&mutter)
                .expect("mutter metadata")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&mutter, permissions).expect("mutter permissions");
        }
        assert!(command_exists_in_path("mutter", tempdir.path().as_os_str()));
    }

    #[test]
    fn unsupported_host_exit_code_is_distinct_from_test_failure() {
        assert_ne!(UNSUPPORTED_HOST_EXIT_CODE, TEST_FAILURE_EXIT_CODE);
        assert_ne!(UNSUPPORTED_HOST_EXIT_CODE, 0);
        assert_eq!(status_to_exit_code(Some(0)), ExitCode::from(0));
        assert_eq!(
            status_to_exit_code(Some(i32::from(TEST_FAILURE_EXIT_CODE))),
            ExitCode::from(TEST_FAILURE_EXIT_CODE)
        );
    }

    #[test]
    fn child_environment_sets_markers_and_removes_live_display() {
        let config = HarnessConfig::new("APP_CHILD", "APP_HEADLESS", "APP_MONITOR");
        let mut command = Command::new("true");

        apply_headless_child_environment(&mut command, &config);

        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.iter().any(|(key, value)| {
            *key == std::ffi::OsStr::new("GDK_BACKEND")
                && value.is_some_and(|value| value == std::ffi::OsStr::new("wayland"))
        }));
        assert!(envs.iter().any(|(key, value)| {
            *key == std::ffi::OsStr::new("APP_HEADLESS")
                && value.is_some_and(|value| value == std::ffi::OsStr::new("1"))
        }));
        assert!(
            envs.iter()
                .any(|(key, value)| *key == std::ffi::OsStr::new("DISPLAY") && value.is_none())
        );
        assert!(envs.iter().any(|(key, value)| {
            *key == std::ffi::OsStr::new("WAYLAND_DISPLAY") && value.is_none()
        }));
        assert_eq!(config.with_test_attempts(0).attempts(), 1);
    }

    #[test]
    fn flake_report_text_avoids_warning_terms() {
        let text = "FLAKY SUMMARY: 1 widget test(s) passed only on retry - investigate and fix the root cause:";
        assert!(!text.contains("warning"));
        assert!(!text.contains("WARNING"));
        assert!(!text.contains("CRITICAL"));
    }

    #[test]
    fn wait_until_dispatches_local_idle_completion() {
        let done = Rc::new(Cell::new(false));
        glib::idle_add_local_once({
            let done = Rc::clone(&done);
            move || done.set(true)
        });

        wait_until(Duration::from_secs(1), || done.get());
    }
}
