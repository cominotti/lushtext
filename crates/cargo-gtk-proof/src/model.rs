// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed validation for the current proof schema descriptors.
//!
//! The live Python visual runner remains the execution oracle in this phase,
//! but Rust owns enough structure to reject stale, malformed, or future-version
//! artifacts before policy checks trust them.

use std::fmt;

use gtk_lush_proof_spine::ProofStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const SUPPORTED_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ValidationOutcome {
    pub(crate) kind: DocumentKind,
    pub(crate) schema_version: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentKind {
    VisualScenario,
    ExpandedCase,
    RootSummary,
    ComparisonReport,
    AnimationReport,
    ProofPolicy,
    ArtifactEnvelope,
    GenericVersioned,
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::VisualScenario => "visual-scenario",
            Self::ExpandedCase => "expanded-case",
            Self::RootSummary => "root-summary",
            Self::ComparisonReport => "comparison-report",
            Self::AnimationReport => "animation-report",
            Self::ProofPolicy => "proof-policy",
            Self::ArtifactEnvelope => "artifact-envelope",
            Self::GenericVersioned => "generic-versioned",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ValidationError {
    pub(crate) status: ProofStatus,
    pub(crate) detail: String,
}

impl ValidationError {
    fn unsupported(version: u64) -> Self {
        Self {
            status: ProofStatus::UnsupportedSchemaVersion,
            detail: format!("unsupported schema_version {version}"),
        }
    }

    fn malformed(detail: impl Into<String>) -> Self {
        Self {
            status: ProofStatus::MalformedField,
            detail: detail.into(),
        }
    }
}

pub(crate) fn validate_document(value: &Value) -> Result<ValidationOutcome, ValidationError> {
    let schema_version = schema_version(value)?;
    if schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ValidationError::unsupported(schema_version));
    }

    let kind = detect_document_kind(value);
    match kind {
        DocumentKind::VisualScenario => validate_visual_scenario(value)?,
        DocumentKind::ExpandedCase => {
            let _case = validate_deserializes::<ExpandedCase>(value, kind)?;
        }
        DocumentKind::RootSummary => {
            let _summary = validate_deserializes::<RootSummary>(value, kind)?;
        }
        DocumentKind::ComparisonReport => {
            let _report = validate_deserializes::<ComparisonReport>(value, kind)?;
        }
        DocumentKind::AnimationReport => {
            let _report = validate_deserializes::<AnimationReport>(value, kind)?;
        }
        DocumentKind::ProofPolicy => {
            let _policy = validate_deserializes::<ProofPolicyMetadata>(value, kind)?;
        }
        DocumentKind::ArtifactEnvelope => validate_artifact_envelope(value)?,
        DocumentKind::GenericVersioned => {}
    }

    Ok(ValidationOutcome {
        kind,
        schema_version,
    })
}

fn schema_version(value: &Value) -> Result<u64, ValidationError> {
    value
        .get("schema_version")
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .get("version")
                .and_then(|version| version.get("schema_version"))
                .and_then(Value::as_u64)
        })
        .ok_or_else(|| ValidationError::malformed("missing schema_version"))
}

fn detect_document_kind(value: &Value) -> DocumentKind {
    if value.get("scenario_id").is_some() && value.get("matrix").is_some() {
        DocumentKind::VisualScenario
    } else if value.get("case_id").is_some() && value.get("manifest").is_some() {
        DocumentKind::ExpandedCase
    } else if value.get("case_count").is_some() && value.get("cases").is_some() {
        DocumentKind::RootSummary
    } else if value.get("animation_frame_evidence").is_some() || value.get("frames").is_some() {
        DocumentKind::AnimationReport
    } else if value.get("protected_regions").is_some()
        || value.get("pixel_anchor_evidence").is_some()
    {
        DocumentKind::ComparisonReport
    } else if value.get("changed_files_digest").is_some()
        || value.get("required_invariant_ids").is_some()
        || value.get("visual_sensitive_changes").is_some()
    {
        DocumentKind::ProofPolicy
    } else if value.get("ok").is_some() && value.get("command").is_some() {
        DocumentKind::ArtifactEnvelope
    } else {
        DocumentKind::GenericVersioned
    }
}

fn validate_visual_scenario(value: &Value) -> Result<(), ValidationError> {
    let manifest =
        validate_deserializes::<VisualScenarioManifest>(value, DocumentKind::VisualScenario)?;
    if manifest.scenario_id.trim().is_empty() {
        return Err(ValidationError::malformed("scenario_id must not be empty"));
    }
    if manifest.scenario_type.trim().is_empty() {
        return Err(ValidationError::malformed(
            "scenario_type must not be empty",
        ));
    }
    if manifest.matrix.sizes.is_empty() {
        return Err(ValidationError::malformed("matrix.sizes must not be empty"));
    }
    if manifest.matrix.color_schemes.is_empty() {
        return Err(ValidationError::malformed(
            "matrix.color_schemes must not be empty",
        ));
    }
    if manifest.protected_regions.is_empty() {
        return Err(ValidationError::malformed(
            "protected_regions must not be empty",
        ));
    }
    if !manifest.pixel_anchors.is_empty() && manifest.invariant_id.is_none() {
        return Err(ValidationError::malformed(
            "pixel_anchors require invariant_id",
        ));
    }
    if let Some(animation) = &manifest.animation_sampling {
        animation.validate(&manifest)?;
    }
    Ok(())
}

fn validate_artifact_envelope(value: &Value) -> Result<(), ValidationError> {
    let envelope =
        validate_deserializes::<ArtifactEnvelopeShape>(value, DocumentKind::ArtifactEnvelope)?;
    if envelope.command.trim().is_empty() {
        return Err(ValidationError::malformed("command must not be empty"));
    }
    if envelope.detail.trim().is_empty() {
        return Err(ValidationError::malformed("detail must not be empty"));
    }
    Ok(())
}

fn validate_deserializes<T>(value: &Value, kind: DocumentKind) -> Result<T, ValidationError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value.clone())
        .map_err(|error| ValidationError::malformed(format!("{kind} malformed field: {error}")))
}

#[derive(Debug, Deserialize, Serialize)]
struct VisualScenarioManifest {
    schema_version: u64,
    scenario_id: String,
    scenario_type: String,
    #[serde(default)]
    invariant_id: Option<String>,
    #[serde(default)]
    description: Option<String>,
    matrix: ScenarioMatrix,
    protected_regions: Vec<ProtectedRegion>,
    #[serde(default)]
    pixel_anchors: Vec<PixelAnchor>,
    #[serde(default)]
    relative_pixel_anchors: Vec<RelativePixelAnchor>,
    #[serde(default)]
    allowed_changing_regions: Vec<AllowedChangingRegion>,
    #[serde(default)]
    readiness_predicates: Vec<String>,
    #[serde(default)]
    animation_sampling: Option<AnimationSampling>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ScenarioMatrix {
    sizes: Vec<ViewportSize>,
    color_schemes: Vec<String>,
    #[serde(default)]
    word_wrap: Vec<bool>,
    #[serde(default)]
    directions: Vec<String>,
    #[serde(default)]
    viewport_positions: Vec<String>,
    #[serde(default)]
    fixture_kinds: Vec<String>,
    #[serde(default)]
    exclude: Vec<MatrixExclusion>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ViewportSize {
    id: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct MatrixExclusion {
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    color_scheme: Option<String>,
    #[serde(default)]
    word_wrap: Option<bool>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    viewport_position: Option<String>,
    #[serde(default)]
    fixture_kind: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProtectedRegion {
    name: String,
    surface: String,
    comparison: String,
    #[serde(default)]
    require_same_rect: bool,
    #[serde(default)]
    mask_rects: Vec<Rect>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Rect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct PixelAnchor {
    name: String,
    crop_surface: String,
    detector: String,
    min_pixels: u32,
    #[serde(default)]
    min_row_offset: Option<i32>,
    #[serde(default)]
    max_screen_y_delta: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RelativePixelAnchor {
    from: String,
    to: String,
    #[serde(default)]
    min_delta: Option<i32>,
    #[serde(default)]
    max_delta: Option<i32>,
    #[serde(default)]
    max_delta_change: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AllowedChangingRegion {
    surface: String,
    relationship: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AnimationSampling {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    capture_mode: Option<String>,
    #[serde(default)]
    invariant_id: Option<String>,
    #[serde(default)]
    stream_frame_count: Option<u32>,
    #[serde(default)]
    stream_timeout_ms: Option<u64>,
    #[serde(default)]
    sample_interval_ms: Option<u64>,
    #[serde(default)]
    sample_count: Option<u32>,
    #[serde(default)]
    max_sample_skew_ms: Option<u64>,
    #[serde(default)]
    max_screen_y_delta: Option<i32>,
    #[serde(default)]
    require_intermediate_geometry: bool,
    #[serde(default)]
    required_anchors: Vec<String>,
}

impl AnimationSampling {
    fn validate(&self, manifest: &VisualScenarioManifest) -> Result<(), ValidationError> {
        if !self.enabled {
            return Ok(());
        }
        if manifest.pixel_anchors.is_empty() {
            return Err(ValidationError::malformed(
                "animation_sampling requires pixel_anchors",
            ));
        }
        if self.invariant_id.as_deref().unwrap_or_default().is_empty() {
            return Err(ValidationError::malformed(
                "animation_sampling requires invariant_id",
            ));
        }
        if self.sample_count == Some(0) {
            return Err(ValidationError::malformed(
                "animation_sampling sample_count must be positive",
            ));
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize)]
struct ExpandedCase {
    schema_version: u64,
    case_id: String,
    manifest: Value,
    size: ViewportSize,
    color_scheme: String,
    artifact_dir: String,
    #[serde(default)]
    word_wrap: Option<bool>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    viewport_position: Option<String>,
    #[serde(default)]
    fixture_kind: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RootSummary {
    schema_version: u64,
    status: String,
    case_count: u64,
    passed: u64,
    failed: u64,
    skipped: u64,
    #[serde(default)]
    case_filter: Option<String>,
    #[serde(default)]
    verified_invariant_ids: Vec<String>,
    #[serde(default)]
    pixel_verified_invariant_ids: Vec<String>,
    #[serde(default)]
    animation_verified_invariant_ids: Vec<String>,
    #[serde(default)]
    pixel_anchor_assertion_count: u64,
    #[serde(default)]
    animation_frame_sample_count: u64,
    #[serde(default)]
    visual_proof_policy: Option<ProofPolicyMetadata>,
    cases: Vec<CaseSummary>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CaseSummary {
    case_id: String,
    status: String,
    #[serde(default)]
    failure_status: Option<String>,
    #[serde(default)]
    invariant_id: Option<String>,
    #[serde(default)]
    pixel_anchor_assertion_count: u64,
    #[serde(default)]
    pixel_verified_invariant_ids: Vec<String>,
    #[serde(default)]
    animation_verified_invariant_ids: Vec<String>,
    #[serde(default)]
    animation_frame_sample_count: u64,
    artifact_dir: String,
    manifest: String,
    #[serde(default)]
    final_geometry: Option<Value>,
    #[serde(default)]
    pixel_anchor_evidence: Vec<Value>,
    #[serde(default)]
    app_vs_rendered_disagreements: Vec<Value>,
    #[serde(default)]
    rendered_anchor_stability: Vec<Value>,
    #[serde(default)]
    animation_frame_evidence: Option<AnimationFrameEvidence>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ComparisonReport {
    schema_version: u64,
    status: String,
    #[serde(default)]
    protected_regions: Vec<Value>,
    #[serde(default)]
    pixel_anchor_evidence: Vec<Value>,
    #[serde(default)]
    rendered_anchor_stability: Vec<Value>,
    #[serde(default)]
    app_vs_rendered_disagreements: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AnimationReport {
    schema_version: u64,
    status: String,
    #[serde(default)]
    invariant_id: Option<String>,
    #[serde(default)]
    frames: Vec<AnimationFrame>,
    #[serde(default)]
    animation_frame_evidence: Option<AnimationFrameEvidence>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AnimationFrameEvidence {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    invariant_id: Option<String>,
    #[serde(default)]
    frames: Vec<AnimationFrame>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AnimationFrame {
    #[serde(default)]
    frame_index: Option<u64>,
    #[serde(default)]
    elapsed_ms: Option<u64>,
    #[serde(default)]
    sample_elapsed_ms: Option<u64>,
    #[serde(default)]
    sidebar_phase: Option<String>,
    #[serde(default)]
    anchors: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProofPolicyMetadata {
    schema_version: u64,
    #[serde(default)]
    changed_files_digest: Option<String>,
    #[serde(default)]
    visual_sensitive_changes: Vec<String>,
    #[serde(default)]
    required_invariant_ids: Vec<String>,
    #[serde(default)]
    required_animation_invariant_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ArtifactEnvelopeShape {
    ok: bool,
    status: String,
    command: String,
    detail: String,
    version: VersionShape,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct VersionShape {
    schema_version: u64,
    #[serde(default)]
    interface_version: Option<String>,
    #[serde(default)]
    tool_version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_current_visual_scenario_manifest() {
        let value: Value = serde_json::from_str(include_str!(
            "../../../scripts/visual-geometry-scenarios/command-palette-overlay.json"
        ))
        .expect("scenario json");

        let outcome = validate_document(&value).expect("valid scenario");

        assert_eq!(outcome.kind, DocumentKind::VisualScenario);
        assert_eq!(outcome.schema_version, SUPPORTED_SCHEMA_VERSION);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let value = serde_json::json!({
            "schema_version": 999,
            "scenario_id": "future",
            "scenario_type": "minimap-sidebar",
            "matrix": { "sizes": [], "color_schemes": [] },
            "protected_regions": []
        });

        let error = validate_document(&value).expect_err("unsupported schema");

        assert_eq!(error.status, ProofStatus::UnsupportedSchemaVersion);
    }

    #[test]
    fn rejects_malformed_scenario_fields() {
        let value = serde_json::json!({
            "schema_version": 1,
            "scenario_id": "broken",
            "scenario_type": "minimap-sidebar",
            "matrix": { "sizes": [], "color_schemes": ["force-light"] },
            "protected_regions": []
        });

        let error = validate_document(&value).expect_err("malformed scenario");

        assert_eq!(error.status, ProofStatus::MalformedField);
        assert!(error.detail.contains("matrix.sizes"));
    }
}
