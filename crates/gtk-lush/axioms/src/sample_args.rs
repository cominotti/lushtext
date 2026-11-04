// SPDX-License-Identifier: MIT OR Apache-2.0

//! How a sample reads its command line, decided before anything touches GTK.
//!
//! A sample run without `--check` opens its interactive window on whatever
//! desktop session it is started from. So a request for help, or a flag the
//! sample does not know, must never fall through to that window: `--help`
//! and `-h` print the usage and exit `0`, and an unknown argument prints the
//! usage and exits `2`, both before GTK is initialized. A sample's own
//! measurement modes are named in its [`SampleFlags`]; their options are
//! accepted only beside a mode.

/// The measurement modes and their options one sample accepts, beside the
/// shared `--check`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SampleFlags {
    /// Flags that select a measurement mode (`--name` or `--name=value`).
    pub modes: &'static [&'static str],
    /// Flags that only modify a mode (`--name` or `--name=value`).
    pub options: &'static [&'static str],
}

impl SampleFlags {
    /// A sample with no measurement mode.
    pub const NONE: Self = Self {
        modes: &[],
        options: &[],
    };
}

/// What a sample's command line asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SampleRequest {
    /// Print the usage and exit `0`.
    Help,
    /// Run the probe headless (`--check`).
    Check,
    /// Run the named measurement mode.
    Mode(&'static str),
    /// No arguments: the interactive window.
    Interactive,
    /// An argument the sample does not know, or an option without its mode:
    /// print the usage and exit `2`.
    Unknown(String),
}

fn flag_name(argument: &str) -> &str {
    argument.split_once('=').map_or(argument, |(name, _)| name)
}

/// Classify `arguments` (without the program name) against `flags`.
#[must_use]
pub fn classify_sample_args(arguments: &[String], flags: &SampleFlags) -> SampleRequest {
    if arguments.iter().any(|a| a == "--help" || a == "-h") {
        return SampleRequest::Help;
    }
    let mode = arguments
        .iter()
        .find_map(|a| flags.modes.iter().find(|m| **m == flag_name(a)).copied());
    let mut check = false;
    for argument in arguments {
        let name = flag_name(argument);
        let known = if name == "--check" {
            check = true;
            mode.is_none()
        } else {
            flags.modes.contains(&name) || (mode.is_some() && flags.options.contains(&name))
        };
        if !known {
            return SampleRequest::Unknown(argument.clone());
        }
    }
    match (mode, check) {
        (Some(mode), _) => SampleRequest::Mode(mode),
        (None, true) => SampleRequest::Check,
        (None, false) => SampleRequest::Interactive,
    }
}

/// The usage text of the sample `program` with `flags`.
#[must_use]
pub fn sample_usage(program: &str, flags: &SampleFlags) -> String {
    let mut text = format!(
        "usage: {program} [--check | --help]\n\n  (no argument)  open the interactive window on the current desktop session\n  --check        run the probe and print its verdict; only inside a private headless\n                 session (`make gtk-axiom-sample AXIOM=<id> CHECK=1`)\n  -h, --help     print this text\n"
    );
    if !flags.modes.is_empty() {
        text.push_str(&format!(
            "\nmeasurement modes (headless only; see the sample's doc comment): {}\n",
            flags.modes.join(", ")
        ));
        if !flags.options.is_empty() {
            text.push_str(&format!("mode options: {}\n", flags.options.join(", ")));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAGS: SampleFlags = SampleFlags {
        modes: &["--derive"],
        options: &["--repetitions"],
    };

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn help_wins_over_everything_and_never_opens_the_window() {
        for list in [
            &["--help"][..],
            &["-h"],
            &["--check", "--help"],
            &["--derive", "-h"],
            &["--bogus", "--help"],
        ] {
            assert_eq!(
                classify_sample_args(&args(list), &FLAGS),
                SampleRequest::Help,
                "{list:?}"
            );
        }
    }

    #[test]
    fn unknown_arguments_are_refused() {
        assert_eq!(
            classify_sample_args(&args(&["--bogus"]), &SampleFlags::NONE),
            SampleRequest::Unknown("--bogus".into())
        );
        assert_eq!(
            classify_sample_args(&args(&["positional"]), &SampleFlags::NONE),
            SampleRequest::Unknown("positional".into())
        );
        // An option without its mode would otherwise reach the window.
        assert_eq!(
            classify_sample_args(&args(&["--repetitions=3"]), &FLAGS),
            SampleRequest::Unknown("--repetitions=3".into())
        );
        assert_eq!(
            classify_sample_args(&args(&["--derive", "--check"]), &FLAGS),
            SampleRequest::Unknown("--check".into())
        );
    }

    #[test]
    fn known_requests_are_classified() {
        assert_eq!(
            classify_sample_args(&[], &FLAGS),
            SampleRequest::Interactive
        );
        assert_eq!(
            classify_sample_args(&args(&["--check"]), &FLAGS),
            SampleRequest::Check
        );
        assert_eq!(
            classify_sample_args(&args(&["--derive=html:1", "--repetitions=2"]), &FLAGS),
            SampleRequest::Mode("--derive")
        );
    }
}
