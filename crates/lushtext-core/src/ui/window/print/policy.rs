// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the print workflow's `policy.rs`.
//!
//! Two decisions, both GTK-free so they can be verified without a printer or a
//! widget: the outcome vocabulary the shell reasons in, and whether a finished
//! print request owes the user a message.

/// Result category returned by the production print operation or the test probe.
///
/// This is the workflow's own vocabulary rather than GTK's: the adapter maps
/// `gtk4::PrintOperationResult` onto it once, so every later stage reasons about
/// print outcomes without a GTK type in its signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintOutcome {
    /// GTK accepted or completed the print request.
    Completed,
    /// GTK handed the print request off asynchronously.
    InProgress,
    /// The user canceled the print dialog.
    Cancelled,
    /// Printing failed before the request could complete.
    Failed(String),
}

/// User-visible prefix for a failed print request.
///
/// Pinned as a literal because it is the only text this workflow shows the user;
/// a test asserting the rendered string catches a reworded prefix, which
/// `assert_eq!(x, PRINT_FAILURE_PREFIX)` could not.
pub const PRINT_FAILURE_PREFIX: &str = "Print failed: ";

/// Whether a finished print request owes the user a message, and which one.
///
/// A cancelled print is deliberately silent: the user dismissed the dialog
/// themselves, so a status message would report their own action back to them.
/// `InProgress` is silent because GTK owns the request from that point on and
/// will not tell us how it ended.
#[must_use]
pub fn print_failure_report(outcome: &PrintOutcome) -> Option<String> {
    match outcome {
        PrintOutcome::Completed | PrintOutcome::InProgress | PrintOutcome::Cancelled => None,
        PrintOutcome::Failed(detail) => Some(format!("{PRINT_FAILURE_PREFIX}{detail}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_failed_print_reports_to_the_user() {
        assert_eq!(print_failure_report(&PrintOutcome::Completed), None);
        assert_eq!(print_failure_report(&PrintOutcome::InProgress), None);
        assert_eq!(print_failure_report(&PrintOutcome::Cancelled), None);
    }

    #[test]
    fn a_failed_print_renders_the_pinned_prefix_and_the_backend_detail() {
        let report = print_failure_report(&PrintOutcome::Failed("no printers".to_string()))
            .expect("expected a failed print to report");
        assert_eq!(report, "Print failed: no printers");
        assert!(report.starts_with(PRINT_FAILURE_PREFIX));
        assert!(report.ends_with("no printers"));
    }

    #[test]
    fn the_failure_prefix_is_the_exact_user_visible_literal() {
        assert_eq!(PRINT_FAILURE_PREFIX, "Print failed: ");
    }
}
