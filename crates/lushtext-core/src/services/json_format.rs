// SPDX-License-Identifier: GPL-3.0-or-later

//! Versioned JSON envelope helpers for app-owned persistence files.
//!
//! This module owns only the public on-disk shape. Recovery, quarantine, and
//! durable writes stay in the surrounding service modules so the domain models
//! do not grow persistence metadata fields.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Public JSON format version supported by the current runtime.
///
/// Keeping this as a single constant makes future v2 dispatch explicit instead
/// of letting individual services invent slightly different version rules.
pub const SUPPORTED_JSON_VERSION: u32 = 1;
/// Stable document kind for `workspaces.json`.
pub const KIND_WORKSPACE_STATE: &str = "dev.cominotti.lushtext.workspace-state";
/// Stable document kind for user-managed saved searches.
pub const KIND_SAVED_SEARCHES: &str = "dev.cominotti.lushtext.saved-searches";
/// Stable document kind for low-stakes recent search history.
pub const KIND_SEARCH_HISTORY: &str = "dev.cominotti.lushtext.search-history";
/// Stable document kind for `session.json`.
pub const KIND_SESSION: &str = "dev.cominotti.lushtext.session";
/// Stable document kind for `drafts/manifest.json`.
pub const KIND_DRAFT_MANIFEST: &str = "dev.cominotti.lushtext.draft-manifest";
/// Stable document kind for bookmark sidecars.
pub const KIND_BOOKMARK_SIDECAR: &str = "dev.cominotti.lushtext.bookmark-sidecar";
/// Stable document kind for document-note sidecars.
pub const KIND_DOCUMENT_NOTE_SIDECAR: &str = "dev.cominotti.lushtext.document-note-sidecar";
/// Stable document kind for folder-note sidecars.
pub const KIND_FOLDER_NOTE_SIDECAR: &str = "dev.cominotti.lushtext.folder-note-sidecar";
/// Legacy document kind accepted only when reading pre-rename folder-note sidecars.
pub const KIND_LEGACY_WORKSPACE_NOTE_SIDECAR: &str =
    "dev.cominotti.lushtext.workspace-note-sidecar";
/// Stable document kind for local-history lineage indexes.
pub const KIND_LOCAL_HISTORY_INDEX: &str = "dev.cominotti.lushtext.local-history-index";
/// Stable document kind for the post-rename migration ledger.
pub const KIND_MIGRATION_LEDGER: &str = "dev.cominotti.lushtext.migration-ledger";
/// Stable document kind for a Replace All undo journal manifest.
pub const KIND_REPLACE_UNDO_MANIFEST: &str = "dev.cominotti.lushtext.replace-all-undo-manifest";
/// Stable document kind for one Replace All undo journal entry.
pub const KIND_REPLACE_UNDO_ENTRY: &str = "dev.cominotti.lushtext.replace-all-undo-entry";
/// Stable document kind for the inactive Replace All cleanup marker.
pub const KIND_REPLACE_UNDO_CLEANUP_MARKER: &str =
    "dev.cominotti.lushtext.replace-all-undo-cleanup-marker";
/// Reserved kind used only to classify retired pre-public single-file undo backups.
pub const KIND_RETIRED_REPLACE_UNDO_BACKUP: &str =
    "dev.cominotti.lushtext.retired-replace-all-undo-backup";
/// Stable document kind for format-upgrade backup manifests.
pub const KIND_FORMAT_UPGRADE_BACKUP_MANIFEST: &str =
    "dev.cominotti.lushtext.format-upgrade-backup-manifest";

/// Borrowed representation used when writing a v1 JSON envelope.
#[derive(Serialize)]
pub struct JsonEnvelopeRef<'a, T: ?Sized> {
    /// Stable document kind identifying which metadata class this file stores.
    pub kind: &'static str,
    /// Integer format version. The current public contract supports v1 only.
    pub version: u32,
    /// Domain payload kept separate from envelope metadata.
    pub data: &'a T,
}

impl<'a, T: ?Sized> JsonEnvelopeRef<'a, T> {
    /// Wrap one domain value in the current v1 envelope.
    #[must_use]
    pub const fn new(kind: &'static str, data: &'a T) -> Self {
        Self {
            kind,
            version: SUPPORTED_JSON_VERSION,
            data,
        }
    }
}

/// Parser error after separating JSON syntax failures from unsupported formats.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JsonFormatError {
    /// The bytes are not syntactically valid JSON.
    Malformed { detail: String },
    /// The JSON is valid but not the requested supported document format.
    UnsupportedFormat { detail: String },
    /// The document kind is recognized, but the version is not supported.
    UnsupportedVersion {
        version: u32,
        supported_versions: String,
    },
}

/// Parse a supported v1 envelope and deserialize only the `data` payload.
///
/// A pre-public bare JSON object is deliberately rejected as unsupported format,
/// even if it would deserialize into `T`, because runtime readers are a clean
/// break from those old shapes.
///
/// # Errors
///
/// Returns [`JsonFormatError::Malformed`] for invalid JSON syntax,
/// [`JsonFormatError::UnsupportedFormat`] for bare, wrong-kind, missing-field,
/// or unsupported payload shapes, and [`JsonFormatError::UnsupportedVersion`]
/// for envelopes whose kind matches but version is not supported.
pub fn parse_v1_payload<T>(bytes: &[u8], expected_kind: &'static str) -> Result<T, JsonFormatError>
where
    T: DeserializeOwned,
{
    parse_v1_payload_accepting(bytes, expected_kind, &[])
}

/// Parse a supported v1 envelope, accepting explicit legacy document kinds.
///
/// New writes must still use `expected_kind`; this helper exists for narrow
/// compatibility readers that have a documented pre-rename kind to support.
///
/// # Errors
///
/// Returns the same parse, format, kind, version, and payload errors as
/// [`parse_v1_payload`], with `accepted_legacy_kinds` considered compatible
/// only for the envelope kind check.
pub fn parse_v1_payload_accepting<T>(
    bytes: &[u8],
    expected_kind: &'static str,
    accepted_legacy_kinds: &[&'static str],
) -> Result<T, JsonFormatError>
where
    T: DeserializeOwned,
{
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| JsonFormatError::Malformed {
            detail: error.to_string(),
        })?;

    let object = value
        .as_object()
        .ok_or_else(|| JsonFormatError::UnsupportedFormat {
            detail: format!("expected v1 envelope object for {expected_kind}"),
        })?;

    let Some(kind) = object.get("kind").and_then(serde_json::Value::as_str) else {
        return Err(JsonFormatError::UnsupportedFormat {
            detail: format!("missing string kind for {expected_kind}"),
        });
    };
    let kind_accepted = kind == expected_kind || accepted_legacy_kinds.contains(&kind);
    if !kind_accepted {
        let accepted_detail = if accepted_legacy_kinds.is_empty() {
            expected_kind.to_string()
        } else {
            format!("{expected_kind} or {}", accepted_legacy_kinds.join(", "))
        };
        return Err(JsonFormatError::UnsupportedFormat {
            detail: format!("expected kind {accepted_detail}, found {kind}"),
        });
    }

    let Some(version) = object.get("version").and_then(serde_json::Value::as_u64) else {
        return Err(JsonFormatError::UnsupportedFormat {
            detail: format!("missing integer version for {expected_kind}"),
        });
    };
    let version = u32::try_from(version).map_err(|_| JsonFormatError::UnsupportedVersion {
        version: u32::MAX,
        supported_versions: SUPPORTED_JSON_VERSION.to_string(),
    })?;
    if version != SUPPORTED_JSON_VERSION {
        return Err(JsonFormatError::UnsupportedVersion {
            version,
            supported_versions: SUPPORTED_JSON_VERSION.to_string(),
        });
    }

    let Some(data) = object.get("data") else {
        return Err(JsonFormatError::UnsupportedFormat {
            detail: format!("missing data payload for {expected_kind}"),
        });
    };

    serde_json::from_value(data.clone()).map_err(|error| JsonFormatError::UnsupportedFormat {
        detail: format!("payload for {expected_kind} v{version} is unsupported: {error}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct FixturePayload {
        name: String,
        #[serde(default)]
        optional: bool,
    }

    #[test]
    fn parses_matching_v1_envelope() {
        let bytes = br#"{
            "kind": "dev.cominotti.lushtext.session",
            "version": 1,
            "extra": "ignored",
            "data": { "name": "ok", "unknown": true }
        }"#;

        let value: FixturePayload = parse_v1_payload(bytes, KIND_SESSION).expect("parse v1");

        assert_eq!(
            value,
            FixturePayload {
                name: "ok".to_string(),
                optional: false
            }
        );
    }

    #[test]
    fn rejects_bare_json_as_unsupported_format() {
        let error = parse_v1_payload::<FixturePayload>(br#"{"name":"old"}"#, KIND_SESSION)
            .expect_err("bare JSON must be rejected as unsupported format");

        assert!(matches!(error, JsonFormatError::UnsupportedFormat { .. }));
    }

    #[test]
    fn rejects_unsupported_version_separately() {
        let error = parse_v1_payload::<FixturePayload>(
            br#"{"kind":"dev.cominotti.lushtext.session","version":2,"data":{"name":"future"}}"#,
            KIND_SESSION,
        )
        .expect_err("future envelope version must be rejected as unsupported version");

        assert!(matches!(
            error,
            JsonFormatError::UnsupportedVersion { version: 2, .. }
        ));
    }

    #[test]
    fn parses_explicit_legacy_kind_only_when_accepted() {
        let bytes = br#"{
            "kind": "dev.cominotti.lushtext.legacy-session",
            "version": 1,
            "data": { "name": "old" }
        }"#;

        let value: FixturePayload = parse_v1_payload_accepting(
            bytes,
            KIND_SESSION,
            &["dev.cominotti.lushtext.legacy-session"],
        )
        .expect("accepted legacy kind should parse");
        assert_eq!(
            value,
            FixturePayload {
                name: "old".to_string(),
                optional: false
            }
        );

        let error = parse_v1_payload::<FixturePayload>(bytes, KIND_SESSION)
            .expect_err("unlisted legacy kind should remain unsupported");
        assert!(matches!(error, JsonFormatError::UnsupportedFormat { .. }));
    }
}
