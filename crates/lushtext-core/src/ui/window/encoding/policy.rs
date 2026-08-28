// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the encoding workflow's `policy.rs`.
//!
//! Every word this workflow shows the user, and every decision about whether an
//! option row is already the current policy or may be activated. The census
//! recorded the row's owned pure policy as `model/encoding.rs` (15 consumers →
//! domain, stays), which is true and is **not** the whole answer: the domain
//! module owns the encoding *vocabulary*, while the decisions here are about how
//! this workflow explains that vocabulary in three grouped-row dialogs. Probing
//! separated them; the domain module was not forked, and nothing here duplicates
//! a shared limit.
//!
//! These functions were interleaved with `libadwaita::PreferencesGroup`
//! construction, so none of the copy had mutation coverage even though it is the
//! entire user-facing contract of the Decision And Detail Dialogs rule in
//! `.agents/rules/ui.md`.

use crate::model::encoding::{
    DocumentEncoding, FileHealthFindingKind, InvisibleCharactersMode, LineEnding,
};

/// Selection and activation state for one grouped dialog option row.
///
/// Two booleans that must agree with each other: a row is `selected` when it is
/// the editor's current policy, and `enabled` when activating it would change
/// something. Deciding them together is the point — computing them at each call
/// site is how a picker ends up with a row that shows a checkmark *and* still
/// reapplies the policy it already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChoiceRowState {
    /// Whether the row represents the editor's current policy.
    pub selected: bool,
    /// Whether activating the row should apply a new policy.
    pub enabled: bool,
}

impl ChoiceRowState {
    /// The ordinary case: a row is activatable exactly when it is not current.
    #[must_use]
    pub const fn for_current(selected: bool) -> Self {
        Self {
            selected,
            enabled: !selected,
        }
    }
}

/// Whether an encoding option is the current one, for a reopen or save picker.
#[must_use]
pub fn encoding_choice_state(
    option: DocumentEncoding,
    current: DocumentEncoding,
) -> ChoiceRowState {
    ChoiceRowState::for_current(option == current)
}

/// Whether an invisible-character mode option is the current one.
#[must_use]
pub fn invisible_mode_choice_state(
    option: InvisibleCharactersMode,
    current: InvisibleCharactersMode,
) -> ChoiceRowState {
    ChoiceRowState::for_current(option == current)
}

/// Whether a line-ending option is current, and whether it may still be applied.
///
/// The exception this exists for: when the loaded document has **mixed** line
/// endings, the already-selected style stays activatable, because choosing it is
/// how the user clears the mixed-line-ending warning. Treating it as an inert
/// current-choice row would leave no way to resolve the warning from this dialog.
#[must_use]
pub fn line_ending_choice_state(
    option: LineEnding,
    save_line_ending: LineEnding,
    detected: LineEnding,
) -> ChoiceRowState {
    let selected = option == save_line_ending;
    ChoiceRowState {
        selected,
        enabled: !selected || detected == LineEnding::Mixed,
    }
}

/// Describe what a reopen option does without implying it changes save policy.
#[must_use]
pub fn reopen_encoding_subtitle(encoding: DocumentEncoding, selected: bool) -> String {
    let prefix = if selected {
        "Current opened encoding."
    } else {
        "Reinterpret the file from disk with this encoding."
    };
    format!("{prefix} {}", encoding_description(encoding))
}

/// Describe what a save option does, including lossy-confirmation expectations.
#[must_use]
pub fn save_encoding_subtitle(encoding: DocumentEncoding, selected: bool) -> String {
    let prefix = if selected {
        "Current save encoding."
    } else {
        "Use this encoding on future saves."
    };
    format!("{prefix} {}", encoding_description(encoding))
}

/// Keep encoding option explanations consistent across reopen and save pickers.
#[must_use]
pub fn encoding_description(encoding: DocumentEncoding) -> &'static str {
    match encoding {
        DocumentEncoding::Utf8 => "Standard UTF-8 without a byte-order mark.",
        DocumentEncoding::Utf8Bom => "UTF-8 with a byte-order mark prefix.",
        DocumentEncoding::Windows1252 => {
            "Legacy Windows Western text; unsupported characters need confirmation."
        }
        DocumentEncoding::ShiftJis => {
            "Japanese legacy text; unsupported characters need confirmation."
        }
        DocumentEncoding::Utf16Le => "UTF-16 little-endian with a byte-order mark.",
        DocumentEncoding::Utf16Be => "UTF-16 big-endian with a byte-order mark.",
    }
}

/// Explain the currently detected line-ending state.
#[must_use]
pub fn opened_line_ending_subtitle(line_ending: LineEnding) -> String {
    if line_ending == LineEnding::Mixed {
        "Mixed line endings were detected in the loaded document.".to_string()
    } else {
        format!(
            "{} detected in the loaded document.",
            line_ending_description(line_ending)
        )
    }
}

/// Describe a save-line-ending option and whether it is already selected.
#[must_use]
pub fn line_ending_subtitle(
    line_ending: LineEnding,
    selected: bool,
    detected: LineEnding,
) -> String {
    if selected && detected == LineEnding::Mixed {
        return format!(
            "Currently selected for the next save. Choose it to clear the mixed-line-ending warning. {}",
            line_ending_description(line_ending)
        );
    }
    if selected {
        return format!(
            "Current save style. {}",
            line_ending_description(line_ending)
        );
    }
    format!(
        "Use this style on future saves. {}",
        line_ending_description(line_ending)
    )
}

/// Return the plain-language meaning of one save-capable line-ending style.
#[must_use]
pub fn line_ending_description(line_ending: LineEnding) -> &'static str {
    match line_ending {
        LineEnding::Lf => "Unix-style line feed.",
        LineEnding::Crlf => "Windows-style carriage return plus line feed.",
        LineEnding::Cr => "Legacy carriage-return-only line endings.",
        LineEnding::Mixed => "Mixed line endings cannot be written as a save style.",
    }
}

/// Describe one invisible-character display mode in row-subtitle form.
#[must_use]
pub fn invisible_mode_subtitle(mode: InvisibleCharactersMode, selected: bool) -> String {
    let prefix = if selected {
        "Current display mode."
    } else {
        "Switch to this display mode."
    };
    format!("{prefix} {}", invisible_mode_description(mode))
}

/// Return the plain-language meaning of one invisible-character display mode.
#[must_use]
pub fn invisible_mode_description(mode: InvisibleCharactersMode) -> &'static str {
    match mode {
        InvisibleCharactersMode::Off => "Hide whitespace and hidden-character markers.",
        InvisibleCharactersMode::WhitespaceOnly => "Draw spaces and tabs with editor markers.",
        InvisibleCharactersMode::All => {
            "Draw whitespace plus supported hidden-character and BOM markers."
        }
    }
}

/// The invisible-character display modes offered by the picker, in order.
pub const INVISIBLE_MODE_CHOICES: [InvisibleCharactersMode; 3] = [
    InvisibleCharactersMode::Off,
    InvisibleCharactersMode::WhitespaceOnly,
    InvisibleCharactersMode::All,
];

/// Whether one file-health finding is a hidden-character issue.
///
/// These three kinds are the ones the "All" invisible-character mode actually
/// draws markers for, which is why switching to that mode points the user at
/// File Health only when one of them is present.
#[must_use]
pub fn is_hidden_character_finding(kind: FileHealthFindingKind) -> bool {
    matches!(
        kind,
        FileHealthFindingKind::Utf8Bom
            | FileHealthFindingKind::NonBreakingSpace
            | FileHealthFindingKind::ZeroWidthCharacter
    )
}

/// Copy shown when a document has no recorded file-health findings.
pub const NO_FILE_HEALTH_FINDINGS_BODY: &str =
    "No file-health issues are currently recorded for this document.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_current_choice_is_selected_and_inert() {
        let state = ChoiceRowState::for_current(true);
        assert!(state.selected);
        assert!(!state.enabled, "reapplying the current policy is a no-op");

        let other = ChoiceRowState::for_current(false);
        assert!(!other.selected);
        assert!(other.enabled);
    }

    #[test]
    fn encoding_choices_are_current_only_for_the_matching_encoding() {
        let state = encoding_choice_state(DocumentEncoding::Utf8, DocumentEncoding::Utf8);
        assert_eq!(
            state,
            ChoiceRowState {
                selected: true,
                enabled: false
            }
        );
        let other = encoding_choice_state(DocumentEncoding::ShiftJis, DocumentEncoding::Utf8);
        assert_eq!(
            other,
            ChoiceRowState {
                selected: false,
                enabled: true
            }
        );
    }

    #[test]
    fn invisible_mode_choices_are_current_only_for_the_matching_mode() {
        assert_eq!(
            invisible_mode_choice_state(InvisibleCharactersMode::All, InvisibleCharactersMode::All),
            ChoiceRowState {
                selected: true,
                enabled: false
            }
        );
        assert_eq!(
            invisible_mode_choice_state(InvisibleCharactersMode::Off, InvisibleCharactersMode::All),
            ChoiceRowState {
                selected: false,
                enabled: true
            }
        );
    }

    #[test]
    fn a_mixed_document_keeps_its_current_line_ending_activatable() {
        // Choosing the already-selected style is how the user clears the
        // mixed-line-ending warning, so it must stay activatable.
        let state = line_ending_choice_state(LineEnding::Lf, LineEnding::Lf, LineEnding::Mixed);
        assert!(state.selected);
        assert!(
            state.enabled,
            "a mixed document must let the user reapply the current style"
        );
    }

    #[test]
    fn a_clean_document_makes_its_current_line_ending_inert() {
        let state = line_ending_choice_state(LineEnding::Lf, LineEnding::Lf, LineEnding::Lf);
        assert!(state.selected);
        assert!(!state.enabled);
    }

    #[test]
    fn a_non_current_line_ending_is_always_activatable() {
        for detected in [
            LineEnding::Lf,
            LineEnding::Crlf,
            LineEnding::Cr,
            LineEnding::Mixed,
        ] {
            let state = line_ending_choice_state(LineEnding::Crlf, LineEnding::Lf, detected);
            assert!(!state.selected);
            assert!(state.enabled);
        }
    }

    #[test]
    fn every_encoding_has_a_distinct_non_empty_description() {
        let mut seen = Vec::new();
        for encoding in DocumentEncoding::COMMON {
            let description = encoding_description(encoding);
            assert!(!description.is_empty());
            assert!(description.ends_with('.'), "{description}");
            assert!(!seen.contains(&description), "duplicate: {description}");
            seen.push(description);
        }
    }

    #[test]
    fn reopen_and_save_subtitles_say_different_things_about_the_same_encoding() {
        let reopen = reopen_encoding_subtitle(DocumentEncoding::ShiftJis, false);
        let save = save_encoding_subtitle(DocumentEncoding::ShiftJis, false);
        assert!(reopen.starts_with("Reinterpret the file from disk with this encoding."));
        assert!(save.starts_with("Use this encoding on future saves."));
        assert_ne!(
            reopen, save,
            "a reopen must not read as a save policy change"
        );
        // Both still carry the shared description.
        let description = encoding_description(DocumentEncoding::ShiftJis);
        assert!(reopen.ends_with(description));
        assert!(save.ends_with(description));
    }

    #[test]
    fn selected_encoding_subtitles_name_the_current_state() {
        assert!(
            reopen_encoding_subtitle(DocumentEncoding::Utf8, true)
                .starts_with("Current opened encoding.")
        );
        assert!(
            save_encoding_subtitle(DocumentEncoding::Utf8, true)
                .starts_with("Current save encoding.")
        );
    }

    #[test]
    fn a_mixed_opened_document_says_so_rather_than_naming_a_style() {
        let subtitle = opened_line_ending_subtitle(LineEnding::Mixed);
        assert_eq!(
            subtitle,
            "Mixed line endings were detected in the loaded document."
        );
        assert!(
            opened_line_ending_subtitle(LineEnding::Lf).contains("Unix-style line feed."),
            "a clean document names its detected style"
        );
    }

    #[test]
    fn a_selected_line_ending_on_a_mixed_document_explains_how_to_clear_the_warning() {
        let subtitle = line_ending_subtitle(LineEnding::Lf, true, LineEnding::Mixed);
        assert!(subtitle.contains("clear the mixed-line-ending warning"));

        let clean = line_ending_subtitle(LineEnding::Lf, true, LineEnding::Lf);
        assert!(clean.starts_with("Current save style."));
        assert!(!clean.contains("clear the mixed-line-ending warning"));

        let other = line_ending_subtitle(LineEnding::Crlf, false, LineEnding::Lf);
        assert!(other.starts_with("Use this style on future saves."));
    }

    #[test]
    fn mixed_is_described_as_unwritable_rather_than_as_a_save_style() {
        assert_eq!(
            line_ending_description(LineEnding::Mixed),
            "Mixed line endings cannot be written as a save style."
        );
    }

    #[test]
    fn invisible_mode_subtitles_distinguish_current_from_switchable() {
        assert!(
            invisible_mode_subtitle(InvisibleCharactersMode::All, true)
                .starts_with("Current display mode.")
        );
        assert!(
            invisible_mode_subtitle(InvisibleCharactersMode::All, false)
                .starts_with("Switch to this display mode.")
        );
    }

    #[test]
    fn every_invisible_mode_has_a_distinct_non_empty_description() {
        // The subtitle test above asserts only the prefix, which left the
        // description itself uncovered — a mutation returning "" or a constant
        // survived. These are the only words explaining what each mode draws.
        let mut seen = Vec::new();
        for mode in INVISIBLE_MODE_CHOICES {
            let description = invisible_mode_description(mode);
            assert!(!description.is_empty(), "{mode:?} has no description");
            assert!(description.ends_with('.'), "{description}");
            assert!(!seen.contains(&description), "duplicate: {description}");
            seen.push(description);
        }
        assert_eq!(
            invisible_mode_description(InvisibleCharactersMode::Off),
            "Hide whitespace and hidden-character markers."
        );
        assert_eq!(
            invisible_mode_description(InvisibleCharactersMode::All),
            "Draw whitespace plus supported hidden-character and BOM markers."
        );
        // The subtitle must actually carry the description, not just the prefix.
        assert!(
            invisible_mode_subtitle(InvisibleCharactersMode::All, false)
                .ends_with(invisible_mode_description(InvisibleCharactersMode::All))
        );
    }

    #[test]
    fn the_invisible_mode_choices_are_off_whitespace_then_all() {
        assert_eq!(
            INVISIBLE_MODE_CHOICES,
            [
                InvisibleCharactersMode::Off,
                InvisibleCharactersMode::WhitespaceOnly,
                InvisibleCharactersMode::All,
            ]
        );
    }

    #[test]
    fn only_marker_drawing_findings_count_as_hidden_character_issues() {
        assert!(is_hidden_character_finding(FileHealthFindingKind::Utf8Bom));
        assert!(is_hidden_character_finding(
            FileHealthFindingKind::NonBreakingSpace
        ));
        assert!(is_hidden_character_finding(
            FileHealthFindingKind::ZeroWidthCharacter
        ));
        assert!(
            !is_hidden_character_finding(FileHealthFindingKind::MixedLineEndings),
            "mixed line endings draw no hidden-character marker"
        );
    }

    #[test]
    fn the_empty_file_health_copy_is_the_exact_user_visible_literal() {
        assert_eq!(
            NO_FILE_HEALTH_FINDINGS_BODY,
            "No file-health issues are currently recorded for this document."
        );
    }
}
