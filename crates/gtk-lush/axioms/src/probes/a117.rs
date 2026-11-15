// SPDX-License-Identifier: MIT OR Apache-2.0

//! A117: with `GTK_A11Y=test`, `gtk_accessible_announce` on a presented
//! widget **kills the process** (SIGSEGV). `GtkTestATContext` implements no
//! `announce` vfunc and `GtkATContext` gives it no default, so
//! `gtk_at_context_announce` calls a NULL function pointer once the context
//! is realized (GTK 4.24.1 and `main`, `gtk/gtktestatcontext.c`,
//! `gtk/gtkatcontext.c`). With `GTK_A11Y=none` the same call returns: GTK
//! creates no AT context, and `gtk_accessible_announce` has nothing to
//! announce through.
//!
//! GTK also falls back to a `GtkTestATContext` whenever no backend builds a
//! context (`gtk_at_context_create`), so the crash is not confined to test
//! sessions; this probe pins only the `GTK_A11Y=test` and `none` cases.
//!
//! The crashing call cannot run in the probe's own process, so the probe
//! re-executes this executable as a child ([`CONTROL_ENV`] set to `A117`,
//! core dumps off) once per backend, with its output captured. The child
//! presents a label, prints the context type it is about to announce through,
//! announces on the label's window, and prints that it returned. The axiom
//! holds when the `test` child died by SIGSEGV after its first line and the
//! `none` child returned and exited 0, which is the fixture's control.

use std::io::Write as _;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, ExitCode, Stdio};

use gtk4::prelude::*;

use crate::AxiomId;
use crate::observation::{Observation, Recorder};
use crate::probes::session_names::{CONTROL_ENV, labelled_fixture};
use crate::session::{Presented, flush_events};

/// The first word of each line the announcing child prints.
const CHILD_LINE_PREFIX: &str = "gtk-lush-axioms announce:";
/// The signal a NULL function-pointer call raises.
const SIGSEGV: i32 = 11;

/// What one announcing child did.
#[derive(Debug, Eq, PartialEq)]
pub struct AnnounceOutcome {
    /// The AT context type the child announced through, when it got that far
    /// (`None` when it printed no context line).
    pub context_type: Option<String>,
    /// Whether the announcement returned.
    pub returned: bool,
    /// The child's exit code, when it exited.
    pub code: Option<i32>,
    /// The signal that killed the child, when one did.
    pub signal: Option<i32>,
}

impl AnnounceOutcome {
    /// Whether the child was killed by SIGSEGV inside the announcement.
    #[must_use]
    pub fn crashed_in_announce(&self) -> bool {
        self.context_type.is_some() && !self.returned && self.signal == Some(SIGSEGV)
    }

    /// How the child ended, for a readout.
    #[must_use]
    pub fn summary(&self) -> String {
        let end = match (self.code, self.signal) {
            (Some(code), _) => format!("exited {code}"),
            (None, Some(signal)) => format!("killed by signal {signal}"),
            (None, None) => "ended without a status".to_owned(),
        };
        format!(
            "context {}; announcement returned: {}; {end}",
            self.context_type.as_deref().unwrap_or("(none reached)"),
            self.returned
        )
    }
}

/// Parse an announcing child's stdout.
fn parse_child_output(stdout: &str) -> (Option<String>, bool) {
    let mut context_type = None;
    let mut returned = false;
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix(CHILD_LINE_PREFIX) else {
            continue;
        };
        match rest.trim().split_once('=') {
            Some(("context", value)) => context_type = Some(value.to_owned()),
            Some(("returned", "true")) => returned = true,
            _ => {}
        }
    }
    (context_type, returned)
}

/// Run the announcing child with `GTK_A11Y` set to `gtk_a11y`, in this
/// process's display, with core dumps off and its output captured.
///
/// # Errors
///
/// The error text when the child cannot be started.
pub fn announce_in_child(gtk_a11y: &str) -> Result<AnnounceOutcome, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    // `ulimit -c 0` keeps the expected crash from writing a core; `exec`
    // keeps the child's own status, signal included.
    let output = Command::new("sh")
        .arg("-c")
        .arg("ulimit -c 0; exec \"$0\"")
        .arg(executable)
        .env(CONTROL_ENV, AxiomId::new(117).to_string())
        .env("GTK_A11Y", gtk_a11y)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("sh: {error}"))?;
    let (context_type, returned) = parse_child_output(&String::from_utf8_lossy(&output.stdout));
    Ok(AnnounceOutcome {
        context_type,
        returned,
        code: output.status.code(),
        signal: output.status.signal(),
    })
}

/// The announcing child (`GTK_LUSH_AXIOMS_CONTROL=A117`): present a label,
/// report the AT context type, announce on its window, report the return.
pub(crate) fn announce_child() -> ExitCode {
    if let Err(error) = crate::init_toolkit() {
        eprintln!("{CHILD_LINE_PREFIX} GTK did not initialize: {error}");
        return ExitCode::from(2);
    }
    let label = labelled_fixture();
    let shown = Presented::detached(&label);
    let Some(root) = label.root() else {
        eprintln!("{CHILD_LINE_PREFIX} the presented label has no root");
        return ExitCode::from(2);
    };
    // Under `none` GTK has no context, and the binding's non-nullable
    // `at_context()` must not be called.
    let context_type = if std::env::var("GTK_A11Y").as_deref() == Ok("none") {
        "(no context)".to_owned()
    } else {
        root.at_context().type_().name().to_owned()
    };
    println!("{CHILD_LINE_PREFIX} context={context_type}");
    let _ = std::io::stdout().flush();
    root.announce(
        "an announcement from the A117 fixture",
        gtk4::AccessibleAnnouncementPriority::Medium,
    );
    println!("{CHILD_LINE_PREFIX} returned=true");
    let _ = std::io::stdout().flush();
    drop(shown);
    flush_events();
    ExitCode::SUCCESS
}

/// Probe A117. See the module documentation.
#[must_use]
pub fn probe_a117() -> Observation {
    Recorder::run(AxiomId::new(117), |recorder| {
        let none = announce_in_child("none");
        if let Err(error) = &none {
            recorder.measure("none_error", error);
        }
        let none = recorder.require(none.ok(), "control: the GTK_A11Y=none child started")?;
        recorder.measure("none_child", none.summary());
        recorder.control(
            none.context_type.is_some() && none.returned && none.code == Some(0),
            "control: with GTK_A11Y=none the child presents the widget, announces, and exits 0",
        )?;

        let test = announce_in_child("test");
        if let Err(error) = &test {
            recorder.measure("test_error", error);
        }
        let test = recorder.require(test.ok(), "control: the GTK_A11Y=test child started")?;
        recorder.measure("test_child", test.summary());
        recorder.control(
            test.context_type.is_some(),
            "control: the GTK_A11Y=test child presented the widget and reached the announcement",
        )?;
        recorder.axiom(
            test.context_type.as_deref() == Some("GtkTestATContext"),
            "A117: with GTK_A11Y=test the presented window's AT context is a GtkTestATContext",
        )?;
        recorder.axiom(
            test.crashed_in_announce(),
            "A117: with GTK_A11Y=test gtk_accessible_announce kills the process by SIGSEGV",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_output_parses_context_and_return() {
        let output = "noise\ngtk-lush-axioms announce: context=GtkTestATContext\n";
        assert_eq!(
            parse_child_output(output),
            (Some("GtkTestATContext".to_owned()), false)
        );
        let output = format!("{output}gtk-lush-axioms announce: returned=true\n");
        assert_eq!(
            parse_child_output(&output),
            (Some("GtkTestATContext".to_owned()), true)
        );
        assert_eq!(parse_child_output(""), (None, false));
    }

    #[test]
    fn only_a_segfault_after_the_context_line_is_a_crash_in_announce() {
        let crashed = AnnounceOutcome {
            context_type: Some("GtkTestATContext".to_owned()),
            returned: false,
            code: None,
            signal: Some(SIGSEGV),
        };
        assert!(crashed.crashed_in_announce());
        assert!(
            !AnnounceOutcome {
                context_type: None,
                ..crashed
            }
            .crashed_in_announce()
        );
        let returned = AnnounceOutcome {
            context_type: Some("(no context)".to_owned()),
            returned: true,
            code: Some(0),
            signal: None,
        };
        assert!(!returned.crashed_in_announce());
        assert_eq!(
            returned.summary(),
            "context (no context); announcement returned: true; exited 0"
        );
    }
}
