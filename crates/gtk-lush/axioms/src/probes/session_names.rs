// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fixture shared by A115 and A116: which well-known names a GTK client puts
//! on its session bus.
//!
//! Each probe reads `ListNames` from inside the probe, in the axioms session,
//! where the variable under test is set. Its **control** runs the same client
//! without the variable, in a nested private session, so a name that never
//! appears anywhere cannot make the probe pass vacuously, and the control's
//! activations cannot dirty the axioms session's own guard. This crate is a
//! leaf and starts no session itself: the nested session comes from the
//! session helper binary the enclosing session exports as
//! [`SESSION_BIN_ENV`]. The control child is this same executable,
//! re-executed with [`CONTROL_ENV`] set; every sample (`support::run`) and the
//! probe runner call [`run_control_child_if_requested`] first.
//!
//! With the helper binary unset (the runner started by `cargo test` through
//! the harness library) or a control prerequisite missing (the bus has no
//! service file for the name, so nothing could ever activate it), the probe
//! is `FixtureInvalid`, never `Holds`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{gio, glib};

use crate::AxiomId;
use crate::session::{Presented, flush_events, settle, wait_until};

/// The session helper binary an enclosing session exports for nested
/// sessions (`gtk_lush_proof_harness::session::SESSION_BIN_ENV`; this crate
/// cannot depend on the harness, so the name is repeated here).
pub const SESSION_BIN_ENV: &str = "GTK_LUSH_SESSION_BIN";
/// Set to an axiom id (`A115`) in a control child.
pub const CONTROL_ENV: &str = "GTK_LUSH_AXIOMS_CONTROL";
/// The first word of the line a control child prints.
const CONTROL_LINE_PREFIX: &str = "gtk-lush-axioms control:";

/// The AT-SPI bus launcher's well-known name.
pub const A11Y_BUS_NAME: &str = "org.a11y.Bus";
/// The portal frontend's well-known name.
pub const PORTAL_DESKTOP_NAME: &str = "org.freedesktop.portal.Desktop";

/// The axioms whose probe reads the session bus and runs a nested control.
pub const SESSION_BUS_AXIOMS: &[AxiomId] = &[AxiomId::new(115), AxiomId::new(116)];

/// How long the in-session half waits for a name it expects never to see,
/// after presenting its window: long enough for an activation the client
/// started to complete.
pub(crate) const ABSENCE_WINDOW: Duration = Duration::from_millis(1500);
/// How long a control child waits for the name to appear. Activation starts
/// a service (the bus launcher, or the portal frontend and its backends).
const CONTROL_BUDGET: Duration = Duration::from_secs(15);
const CALL_TIMEOUT_MS: i32 = 5_000;

/// The line a sample prints instead of its observation when its probe needs
/// a session bus and runs in an SDK sandbox (`--no-session-bus`):
/// `skipped (needs a session bus): <id>`; `None` for every other axiom.
#[must_use]
pub fn session_bus_skip_line(axiom: AxiomId, sandbox: bool) -> Option<String> {
    (sandbox && SESSION_BUS_AXIOMS.contains(&axiom))
        .then(|| format!("skipped (needs a session bus): {axiom}"))
}

/// The well-known names on this process's session bus.
pub(crate) fn session_bus_names() -> Result<Vec<String>, String> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
        .map_err(|error| error.to_string())?;
    let reply = connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            Some(glib::VariantTy::new("(as)").map_err(|error| error.to_string())?),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| error.to_string())?;
    let names: Vec<String> = reply.child_value(0).get().unwrap_or_default();
    Ok(names
        .into_iter()
        .filter(|name| !name.starts_with(':'))
        .collect())
}

/// Whether a name is on the bus now.
pub(crate) fn name_present(name: &str) -> Result<bool, String> {
    Ok(session_bus_names()?.iter().any(|present| present == name))
}

/// Whether the well-known `name` is on this process's session bus now, for
/// a sample's readout.
///
/// # Errors
///
/// The D-Bus error text when the session bus cannot be reached.
pub fn session_bus_has_name(name: &str) -> Result<bool, String> {
    name_present(name)
}

/// Whether some D-Bus service file in `data_dirs` can activate `name`.
pub(crate) fn service_file_present(name: &str, data_dirs: &[PathBuf]) -> bool {
    data_dirs.iter().any(|dir| {
        dir.join("dbus-1/services")
            .join(format!("{name}.service"))
            .is_file()
    })
}

/// The session bus's activation directories: `XDG_DATA_DIRS`, or its
/// default.
pub(crate) fn data_dirs() -> Vec<PathBuf> {
    let value = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    std::env::split_paths(&value).collect()
}

/// Why the nested control cannot run, or the helper binary to run it with.
pub(crate) fn control_prerequisite<'a>(
    helper: Option<&'a OsStr>,
    name: &str,
    data_dirs: &[PathBuf],
) -> Result<&'a Path, &'static str> {
    let helper = helper.ok_or(
        "control: the enclosing session exports GTK_LUSH_SESSION_BIN (run through \
         scripts/headless-session, as make gtk-axioms does)",
    )?;
    if !service_file_present(name, data_dirs) {
        return Err("control: the session bus has a service file for the name");
    }
    Ok(Path::new(helper))
}

/// What one nested control run reported.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ControlOutcome {
    /// Whether the control child saw the name appear.
    pub(crate) appeared: bool,
    /// The nested session's exit status (`0` when its own guard was clean).
    pub(crate) status: Option<i32>,
}

/// Parse a control child's line, `gtk-lush-axioms control: A115 org.a11y.Bus=true`.
pub(crate) fn parse_control_line(output: &str, axiom: AxiomId, name: &str) -> Option<bool> {
    let wanted = format!("{CONTROL_LINE_PREFIX} {axiom} {name}=");
    output
        .lines()
        .find_map(|line| line.strip_prefix(&wanted))
        .and_then(|value| match value.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

/// Lines of a control run's stderr kept in a failure message.
const CONTROL_STDERR_TAIL_LINES: usize = 8;

/// Run this executable as `axiom`'s control child in a nested `widget`
/// session with `overrides` (`--a11y atspi`, or `--portals on`).
///
/// The control's stderr is captured, not inherited: turning the AT-SPI bus
/// or the portals on is the control's point, and the services it starts warn
/// about the headless session they find (the portal frontend's backend
/// choice, the AT-SPI registry's activation). Those lines are fixture output,
/// not the probe's, and the axiom lane's warning scan must not read them. A
/// failed control reports their tail instead.
pub(crate) fn run_nested_control(
    helper: &Path,
    axiom: AxiomId,
    name: &str,
    overrides: &[&str],
) -> Result<ControlOutcome, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let output = Command::new(helper)
        .args(["run", "--profile", "widget"])
        .args(overrides)
        .arg("--")
        .arg(executable)
        .env(CONTROL_ENV, axiom.to_string())
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("{}: {error}", helper.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let appeared = parse_control_line(&stdout, axiom, name).ok_or_else(|| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines = stderr.lines().collect::<Vec<_>>();
        let tail = lines[lines.len().saturating_sub(CONTROL_STDERR_TAIL_LINES)..].join(" | ");
        format!(
            "the control child printed no result line (status {:?}): stdout {stdout:?}; stderr tail: {tail}",
            output.status.code()
        )
    })?;
    Ok(ControlOutcome {
        appeared,
        status: output.status.code(),
    })
}

/// In a control child, run the control and exit; `None` in every other
/// process.
///
/// A sample's `main` (through `support::run`) and the probe runner call this
/// before anything else, so the child never parses sample arguments or
/// enters the test harness.
#[must_use]
pub fn run_control_child_if_requested() -> Option<ExitCode> {
    let axiom = std::env::var(CONTROL_ENV).ok()?;
    Some(control_child(&axiom))
}

/// `GDK_DEBUG` value that turns off GTK's own portal use (not Libadwaita's).
const GDK_NO_PORTALS: &str = "no-portals";

/// Run this executable again, in the same session, with GTK's portal
/// switches off, and report its status.
fn relaunch_with_gtk_portals_off() -> ExitCode {
    let Ok(current_exe) = std::env::current_exe() else {
        eprintln!("{CONTROL_LINE_PREFIX} cannot find the current executable");
        return ExitCode::from(2);
    };
    match std::process::Command::new(current_exe)
        .args(std::env::args_os().skip(1))
        .env("GDK_DEBUG", GDK_NO_PORTALS)
        .env("GTK_USE_PORTAL", "0")
        .status()
    {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(
            status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(2),
        ),
        Err(error) => {
            eprintln!("{CONTROL_LINE_PREFIX} cannot relaunch the control child: {error}");
            ExitCode::from(2)
        }
    }
}

fn control_child(axiom: &str) -> ExitCode {
    let name = match axiom {
        "A115" => A11Y_BUS_NAME,
        // A117's child announces instead of watching the bus.
        "A117" => return crate::probes::a117::announce_child(),
        "A116" => {
            // The axiom's claim: `GDK_DEBUG=no-portals` alone does not stop
            // Libadwaita. The nested `--portals on` session clears it, so
            // the child relaunches itself with it set back (this crate
            // forbids the unsafe `set_var`), inside the same session.
            if std::env::var("GDK_DEBUG").as_deref() != Ok(GDK_NO_PORTALS) {
                return relaunch_with_gtk_portals_off();
            }
            PORTAL_DESKTOP_NAME
        }
        other => {
            eprintln!("{CONTROL_LINE_PREFIX} unknown axiom {other}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = crate::init_toolkit() {
        eprintln!("{CONTROL_LINE_PREFIX} GTK did not initialize: {error}");
        return ExitCode::from(2);
    }
    let label = labelled_fixture();
    let shown = Presented::detached(&label);
    let appeared = wait_until(CONTROL_BUDGET, || name_present(name).unwrap_or(false));
    println!("{CONTROL_LINE_PREFIX} {axiom} {name}={appeared}");
    drop(shown);
    flush_events();
    ExitCode::SUCCESS
}

/// A label with an accessible property set: the client half of both
/// fixtures.
pub(crate) fn labelled_fixture() -> gtk4::Label {
    let label = gtk4::Label::new(Some("A labelled fixture"));
    label.update_property(&[gtk4::accessible::Property::Description(
        "the accessible property the probe reads back",
    )]);
    label
}

/// Present the fixture and wait out [`ABSENCE_WINDOW`], so an activation the
/// client started has time to finish before `ListNames`.
pub(crate) fn present_and_wait(label: &gtk4::Label) -> Presented {
    let shown = Presented::detached(label);
    settle(ABSENCE_WINDOW);
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dirs(with_service: Option<&str>) -> (tempdir::Scratch, Vec<PathBuf>) {
        let scratch = tempdir::Scratch::new();
        if let Some(name) = with_service {
            let services = scratch.path().join("dbus-1/services");
            std::fs::create_dir_all(&services).expect("create the services directory");
            std::fs::write(
                services.join(format!("{name}.service")),
                "[D-BUS Service]\n",
            )
            .expect("write the service file");
        }
        let dirs = vec![scratch.path().to_path_buf()];
        (scratch, dirs)
    }

    /// A tiny owned temporary directory (the crate has no `tempfile`).
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub(super) struct Scratch(PathBuf);

        impl Scratch {
            pub(super) fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "gtk-lush-axioms-session-names-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).expect("create the scratch directory");
                Self(path)
            }

            pub(super) fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn a_missing_helper_or_service_file_makes_the_control_unavailable() {
        let (_scratch, with) = scratch_dirs(Some(A11Y_BUS_NAME));
        let (_empty, without) = scratch_dirs(None);
        let helper = OsStr::new("/usr/bin/gtk-lush-session");
        assert!(
            control_prerequisite(None, A11Y_BUS_NAME, &with)
                .expect_err("no helper")
                .contains("GTK_LUSH_SESSION_BIN")
        );
        assert!(
            control_prerequisite(Some(helper), A11Y_BUS_NAME, &without)
                .expect_err("no service file")
                .contains("service file")
        );
        assert!(control_prerequisite(Some(helper), PORTAL_DESKTOP_NAME, &with).is_err());
        assert_eq!(
            control_prerequisite(Some(helper), A11Y_BUS_NAME, &with),
            Ok(Path::new("/usr/bin/gtk-lush-session"))
        );
    }

    #[test]
    fn control_lines_parse_only_for_their_axiom_and_name() {
        let output = "noise\ngtk-lush-axioms control: A115 org.a11y.Bus=true\n";
        assert_eq!(
            parse_control_line(output, AxiomId::new(115), A11Y_BUS_NAME),
            Some(true)
        );
        assert_eq!(
            parse_control_line(output, AxiomId::new(116), A11Y_BUS_NAME),
            None
        );
        assert_eq!(
            parse_control_line(
                "gtk-lush-axioms control: A116 org.freedesktop.portal.Desktop=false",
                AxiomId::new(116),
                PORTAL_DESKTOP_NAME
            ),
            Some(false)
        );
        assert_eq!(
            parse_control_line("", AxiomId::new(115), A11Y_BUS_NAME),
            None
        );
    }

    #[test]
    fn session_bus_axioms_skip_only_in_a_sandbox() {
        assert_eq!(
            session_bus_skip_line(AxiomId::new(115), true).as_deref(),
            Some("skipped (needs a session bus): A115")
        );
        assert_eq!(session_bus_skip_line(AxiomId::new(115), false), None);
        assert_eq!(session_bus_skip_line(AxiomId::new(1), true), None);
    }
}
