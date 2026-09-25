// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a probe returns: a verdict, every value it measured, and the toolkit
//! versions it ran against, with a dependency-free one-line JSON encoding.

use std::fmt;

use crate::AxiomId;

/// The outcome of one probe run.
///
/// The three outcomes are deliberately distinct. [`Verdict::FixtureInvalid`]
/// means the probe's control step failed: the fixture never reached the state
/// the axiom talks about, so the run says nothing about whether GTK changed.
/// Reading it as [`Verdict::Violated`] would send a reviewer after a toolkit
/// change that did not happen.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Verdict {
    /// The fixture reached its state and the axiom held.
    Holds,
    /// The fixture reached its state and the axiom did not hold.
    Violated,
    /// A control step failed, so the axiom step was never reached.
    FixtureInvalid,
}

impl Verdict {
    /// The stable lower-case name used in the JSON encoding.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Holds => "holds",
            Self::Violated => "violated",
            Self::FixtureInvalid => "fixture-invalid",
        }
    }

    /// The process exit code a `--check` sample reports for this verdict:
    /// `0` holds, `1` violated, `2` fixture invalid.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Holds => 0,
            Self::Violated => 1,
            Self::FixtureInvalid => 2,
        }
    }
}

/// One probe run: its verdict, what it measured, and where it ran.
///
/// A probe records every value it reads, not only the ones its verdict turns
/// on, so a failure after a toolkit update shows *how* the behaviour moved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    /// The axiom the probe pins.
    pub axiom: AxiomId,
    /// Whether the axiom held.
    pub verdict: Verdict,
    /// Every measured value, in the order the probe read it. When a step
    /// fails, the last entry is `failed_check` naming that step.
    pub measured: Vec<(&'static str, String)>,
    /// The running GTK version, `major.minor.micro`.
    pub gtk_version: String,
    /// The running Libadwaita version, `major.minor.micro`.
    pub adw_version: String,
}

impl Observation {
    /// The value measured under `key`, if the probe recorded one.
    #[cfg(test)]
    fn measured_value(&self, key: &str) -> Option<&str> {
        self.measured
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Encode the observation as one line of JSON with no trailing newline.
    ///
    /// `measured` is an object whose members keep the probe's reading order;
    /// every value is a string, so a reader never has to guess a number's
    /// type or precision.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        let mut line = String::from("{\"axiom\":");
        push_json_string(&mut line, &self.axiom.to_string());
        line.push_str(",\"verdict\":");
        push_json_string(&mut line, self.verdict.as_str());
        line.push_str(",\"measured\":{");
        for (index, (key, value)) in self.measured.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            push_json_string(&mut line, key);
            line.push(':');
            push_json_string(&mut line, value);
        }
        line.push_str("},\"gtk\":");
        push_json_string(&mut line, &self.gtk_version);
        line.push_str(",\"adw\":");
        push_json_string(&mut line, &self.adw_version);
        line.push('}');
        line
    }
}

/// Append `text` as a JSON string literal, escaping per RFC 8259.
fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// The running GTK version, `major.minor.micro`.
fn gtk_version() -> String {
    format!(
        "{}.{}.{}",
        gtk4::major_version(),
        gtk4::minor_version(),
        gtk4::micro_version()
    )
}

/// The running Libadwaita version, `major.minor.micro`.
fn adw_version() -> String {
    format!(
        "{}.{}.{}",
        libadwaita::major_version(),
        libadwaita::minor_version(),
        libadwaita::micro_version()
    )
}

/// Why a probe stopped early: the verdict its failed step implies.
pub(crate) struct Stop(Verdict);

/// Collects a probe's measurements and turns its steps into a verdict.
///
/// A probe body is a sequence of [`Recorder::control`] steps (the fixture
/// reached the state the axiom talks about) followed by [`Recorder::axiom`]
/// steps (the behaviour holds). The first failing step ends the run through
/// `?`, so a probe reads as the chain of assertions it replaced.
pub(crate) struct Recorder {
    measured: Vec<(&'static str, String)>,
}

impl Recorder {
    /// Run `body` for `axiom` and return its observation, stamped with the
    /// running toolkit versions.
    pub(crate) fn run(
        axiom: AxiomId,
        body: impl FnOnce(&mut Self) -> Result<(), Stop>,
    ) -> Observation {
        let mut observation = Self::run_unstamped(axiom, body);
        observation.gtk_version = gtk_version();
        observation.adw_version = adw_version();
        observation
    }

    /// Run `body` without reading the toolkit versions, which needs GTK
    /// initialized; the version fields stay empty.
    fn run_unstamped(
        axiom: AxiomId,
        body: impl FnOnce(&mut Self) -> Result<(), Stop>,
    ) -> Observation {
        let mut recorder = Self {
            measured: Vec::new(),
        };
        let verdict = match body(&mut recorder) {
            Ok(()) => Verdict::Holds,
            Err(Stop(verdict)) => verdict,
        };
        Observation {
            axiom,
            verdict,
            measured: recorder.measured,
            gtk_version: String::new(),
            adw_version: String::new(),
        }
    }

    /// Record one measured value.
    pub(crate) fn measure(&mut self, key: &'static str, value: impl fmt::Display) {
        self.measured.push((key, value.to_string()));
    }

    /// A control step: when it fails the fixture is invalid.
    pub(crate) fn control(&mut self, holds: bool, check: &'static str) -> Result<(), Stop> {
        self.step(holds, check, Verdict::FixtureInvalid)
    }

    /// An axiom step: when it fails the axiom is violated.
    pub(crate) fn axiom(&mut self, holds: bool, check: &'static str) -> Result<(), Stop> {
        self.step(holds, check, Verdict::Violated)
    }

    /// A control step on a value the fixture needs: `Some` passes it
    /// through, `None` fails the step and the fixture is invalid.
    pub(crate) fn require<T>(&mut self, value: Option<T>, check: &'static str) -> Result<T, Stop> {
        value.ok_or_else(|| self.stop(check, Verdict::FixtureInvalid))
    }

    fn step(&mut self, holds: bool, check: &'static str, failure: Verdict) -> Result<(), Stop> {
        if holds {
            Ok(())
        } else {
            Err(self.stop(check, failure))
        }
    }

    /// Record `check` as the failed step and stop with `failure`.
    fn stop(&mut self, check: &'static str, failure: Verdict) -> Stop {
        self.measure("failed_check", check);
        Stop(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(measured: Vec<(&'static str, String)>) -> Observation {
        Observation {
            axiom: AxiomId::new(5),
            verdict: Verdict::Holds,
            measured,
            gtk_version: "4.22.1".to_owned(),
            adw_version: "1.9.0".to_owned(),
        }
    }

    #[test]
    fn json_line_keeps_measurement_order_and_versions() {
        let line = observation(vec![
            ("value_before", "1200".to_owned()),
            ("value_after", "34".to_owned()),
        ])
        .to_json_line();
        assert_eq!(
            line,
            "{\"axiom\":\"A5\",\"verdict\":\"holds\",\"measured\":{\"value_before\":\"1200\",\
             \"value_after\":\"34\"},\"gtk\":\"4.22.1\",\"adw\":\"1.9.0\"}"
        );
    }

    #[test]
    fn json_line_escapes_measured_strings() {
        let line = observation(vec![(
            "failed_check",
            "quote \" backslash \\ newline \n tab \t bell \u{7}".to_owned(),
        )])
        .to_json_line();
        assert!(line.contains(
            "\"failed_check\":\"quote \\\" backslash \\\\ newline \\n tab \\t bell \\u0007\""
        ));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn empty_measurements_encode_as_an_empty_object() {
        assert!(
            observation(Vec::new())
                .to_json_line()
                .contains("\"measured\":{}")
        );
    }

    #[test]
    fn verdict_exit_codes_are_distinct() {
        assert_eq!(Verdict::Holds.exit_code(), 0);
        assert_eq!(Verdict::Violated.exit_code(), 1);
        assert_eq!(Verdict::FixtureInvalid.exit_code(), 2);
        assert_eq!(Verdict::FixtureInvalid.as_str(), "fixture-invalid");
    }

    #[test]
    fn a_failed_control_is_fixture_invalid_and_names_the_step() {
        let observation = Recorder::run_unstamped(AxiomId::new(9), |recorder| {
            recorder.measure("value", 3);
            recorder.control(false, "control: the fixture scrolled")?;
            recorder.axiom(false, "never reached")
        });
        assert_eq!(observation.verdict, Verdict::FixtureInvalid);
        assert_eq!(observation.measured_value("value"), Some("3"));
        assert_eq!(
            observation.measured_value("failed_check"),
            Some("control: the fixture scrolled")
        );
    }

    #[test]
    fn a_failed_axiom_step_is_violated() {
        let observation = Recorder::run_unstamped(AxiomId::new(9), |recorder| {
            recorder.control(true, "control")?;
            recorder.axiom(false, "axiom step")
        });
        assert_eq!(observation.verdict, Verdict::Violated);
        assert_eq!(
            observation.measured_value("failed_check"),
            Some("axiom step")
        );
    }
}
