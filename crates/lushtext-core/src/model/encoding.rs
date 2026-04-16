// SPDX-License-Identifier: GPL-3.0-or-later

//! Domain types for document encoding, line endings, and file-health state.
//!
//! These values stay in `model/` because both background I/O services and GTK
//! adapters need the same vocabulary when they talk about how a document was
//! decoded, how it should be saved, and which encoding-adjacent warnings are
//! active for the current tab.

use encoding_rs::{Encoding, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use std::fmt;

/// Concrete text encodings that LushText can expose in its document workflow.
///
/// The set is intentionally small for the first release: it covers the common
/// reopen/save cases in developer workflows without turning the status-bar UI
/// into an exhaustive legacy-charset picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentEncoding {
    /// UTF-8 without a byte-order mark.
    Utf8,
    /// UTF-8 with a leading byte-order mark on save.
    Utf8Bom,
    /// Windows-1252, the common fallback for "ANSI" text on Windows.
    Windows1252,
    /// Shift_JIS for common Japanese text workflows.
    ShiftJis,
    /// UTF-16 little-endian with a byte-order mark.
    Utf16Le,
    /// UTF-16 big-endian with a byte-order mark.
    Utf16Be,
}

impl DocumentEncoding {
    /// Compact stable identifier used by GTK actions and persisted settings.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf8Bom => "utf-8-bom",
            Self::Windows1252 => "windows-1252",
            Self::ShiftJis => "shift-jis",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
        }
    }

    /// User-facing short label shown in status-bar controls and dialogs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Windows1252 => "Windows-1252",
            Self::ShiftJis => "Shift_JIS",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
        }
    }

    /// Return the `encoding_rs` decoder/encoder for this document encoding.
    ///
    /// UTF-8 with and without BOM share the same codec; BOM handling remains an
    /// explicit policy on top of the byte conversion.
    #[must_use]
    pub fn codec(self) -> &'static Encoding {
        match self {
            Self::Utf8 | Self::Utf8Bom => UTF_8,
            Self::Windows1252 => WINDOWS_1252,
            Self::ShiftJis => SHIFT_JIS,
            Self::Utf16Le => UTF_16LE,
            Self::Utf16Be => UTF_16BE,
        }
    }

    /// Whether saving in this encoding writes a byte-order mark prefix.
    #[must_use]
    pub fn writes_bom(self) -> bool {
        matches!(self, Self::Utf8Bom | Self::Utf16Le | Self::Utf16Be)
    }

    /// Parse a stable identifier back into a document encoding.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "utf-8" => Some(Self::Utf8),
            "utf-8-bom" => Some(Self::Utf8Bom),
            "windows-1252" => Some(Self::Windows1252),
            "shift-jis" => Some(Self::ShiftJis),
            "utf-16le" => Some(Self::Utf16Le),
            "utf-16be" => Some(Self::Utf16Be),
            _ => None,
        }
    }

    /// Shortlist used by the first status-bar encoding picker.
    pub const COMMON: [Self; 6] = [
        Self::Utf8,
        Self::Utf8Bom,
        Self::Windows1252,
        Self::ShiftJis,
        Self::Utf16Le,
        Self::Utf16Be,
    ];
}

impl fmt::Display for DocumentEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// How certain the load pipeline is about the chosen decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeConfidence {
    /// The bytes clearly identified one decoding, for example via BOM or
    /// successful UTF-8 validation.
    Exact,
    /// The bytes matched a heuristic such as UTF-16-without-BOM detection.
    Heuristic,
    /// The bytes required a lossy or weak fallback guess.
    Low,
}

impl DecodeConfidence {
    /// User-facing label for health details.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::Heuristic => "Heuristic",
            Self::Low => "Low",
        }
    }

    /// Whether the confidence is low enough to surface a warning-level finding.
    #[must_use]
    pub fn needs_warning(self) -> bool {
        matches!(self, Self::Heuristic | Self::Low)
    }
}

/// Line-ending styles detected on load or selected for save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix-style line feed.
    Lf,
    /// Windows-style carriage-return + line-feed.
    Crlf,
    /// Legacy carriage-return-only line endings.
    Cr,
    /// Multiple line-ending styles were present in the loaded document.
    Mixed,
}

impl LineEnding {
    /// Stable identifier used by settings and GTK callbacks.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Lf => "lf",
            Self::Crlf => "crlf",
            Self::Cr => "cr",
            Self::Mixed => "mixed",
        }
    }

    /// Compact user-facing label for status-bar controls.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
            Self::Mixed => "Mixed",
        }
    }

    /// The textual separator written to disk for save-capable variants.
    #[must_use]
    pub fn separator(self) -> Option<&'static str> {
        match self {
            Self::Lf => Some("\n"),
            Self::Crlf => Some("\r\n"),
            Self::Cr => Some("\r"),
            Self::Mixed => None,
        }
    }

    /// Parse a stable identifier back into a line-ending variant.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "lf" => Some(Self::Lf),
            "crlf" => Some(Self::Crlf),
            "cr" => Some(Self::Cr),
            "mixed" => Some(Self::Mixed),
            _ => None,
        }
    }

    /// Save-capable line-ending options shown in the picker.
    pub const SAVE_CHOICES: [Self; 3] = [Self::Lf, Self::Crlf, Self::Cr];
}

/// Per-tab invisible-character rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvisibleCharactersMode {
    /// Draw no extra whitespace or anomaly hints.
    #[default]
    Off,
    /// Draw ordinary spaces and tabs using GtkSourceView's native space drawer.
    WhitespaceOnly,
    /// Draw whitespace plus the extra encoding-adjacent markers supported by
    /// the current document workflow.
    All,
}

impl InvisibleCharactersMode {
    /// Stable identifier for settings and action state.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::WhitespaceOnly => "whitespace-only",
            Self::All => "all",
        }
    }

    /// User-facing label shown in status messages and controls.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::WhitespaceOnly => "Whitespace Only",
            Self::All => "All",
        }
    }

    /// Cycle order used by the keyboard shortcut.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::WhitespaceOnly,
            Self::WhitespaceOnly => Self::All,
            Self::All => Self::Off,
        }
    }

    /// Parse a stable identifier back into an invisible-character mode.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "off" => Some(Self::Off),
            "whitespace-only" => Some(Self::WhitespaceOnly),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

/// Severity level for one file-health finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileHealthSeverity {
    /// Informational finding that is useful context but not urgent.
    Info,
    /// Warning-level finding that may require user attention or a fix action.
    Warning,
}

/// Machine-readable categories for file-health findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileHealthFindingKind {
    /// The document carried a UTF-8 BOM.
    Utf8Bom,
    /// The document contained more than one line-ending style.
    MixedLineEndings,
    /// The document opened through a heuristic or low-confidence decoder.
    LowConfidenceDecode,
    /// Raw bytes suggest the file may not be plain text.
    BinaryLikeContent,
    /// The content contains non-breaking spaces.
    NonBreakingSpace,
    /// The content contains zero-width characters.
    ZeroWidthCharacter,
}

/// One surfaced encoding-adjacent health finding for the active document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHealthFinding {
    /// Stable category used by workflow code to look up follow-up actions.
    pub kind: FileHealthFindingKind,
    /// Severity used by the status-bar summary and popover ordering.
    pub severity: FileHealthSeverity,
    /// Short headline for the file-health surface.
    pub title: String,
    /// Longer explanation shown in details surfaces or warning bars.
    pub body: String,
}

/// Per-document encoding and line-ending facts that both the services and the
/// GTK adapter need to agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentEncodingState {
    /// Encoding used to decode the bytes currently shown in the buffer.
    pub opened_encoding: DocumentEncoding,
    /// Encoding to use for the next save.
    pub save_encoding: DocumentEncoding,
    /// Line-ending style observed during the last load.
    pub detected_line_ending: LineEnding,
    /// Line-ending style the next save should write.
    pub save_line_ending: LineEnding,
    /// How certain the load pipeline was about the chosen decoding.
    pub decode_confidence: DecodeConfidence,
}

impl Default for DocumentEncodingState {
    fn default() -> Self {
        Self {
            opened_encoding: DocumentEncoding::Utf8,
            save_encoding: DocumentEncoding::Utf8,
            detected_line_ending: LineEnding::Lf,
            save_line_ending: LineEnding::Lf,
            decode_confidence: DecodeConfidence::Exact,
        }
    }
}

impl DocumentEncodingState {
    /// Summary string for the properties panel when save and open encodings diverge.
    #[must_use]
    pub fn summary(self) -> String {
        if self.opened_encoding == self.save_encoding {
            self.opened_encoding.label().to_string()
        } else {
            format!(
                "{} (save as {})",
                self.opened_encoding.label(),
                self.save_encoding.label()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_encoding_round_trips_ids() {
        for encoding in DocumentEncoding::COMMON {
            assert_eq!(DocumentEncoding::from_id(encoding.id()), Some(encoding));
        }
    }

    #[test]
    fn invisible_mode_cycles_in_order() {
        assert_eq!(
            InvisibleCharactersMode::Off.next(),
            InvisibleCharactersMode::WhitespaceOnly
        );
        assert_eq!(
            InvisibleCharactersMode::WhitespaceOnly.next(),
            InvisibleCharactersMode::All
        );
        assert_eq!(
            InvisibleCharactersMode::All.next(),
            InvisibleCharactersMode::Off
        );
    }

    #[test]
    fn encoding_summary_mentions_save_policy_when_needed() {
        let state = DocumentEncodingState {
            opened_encoding: DocumentEncoding::Utf8,
            save_encoding: DocumentEncoding::Windows1252,
            ..Default::default()
        };
        assert_eq!(state.summary(), "UTF-8 (save as Windows-1252)");
    }
}
