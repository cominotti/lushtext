// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed validation for the current proof schema descriptors.
//!
//! Rust-owned same-session visual proof is the default authority. Python
//! artifacts remain supported only as explicit oracle or compatibility data, so
//! validation keeps rejecting stale, malformed, or future-version artifacts
//! before policy checks trust them.

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
    CaseManifest,
    RootSummary,
    CaseSummary,
    ComparisonReport,
    AnimationReport,
    WarningScan,
    ParityReport,
    EnvironmentReport,
    ProofPolicy,
    ArtifactEnvelope,
    GenericVersioned,
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::VisualScenario => "visual-scenario",
            Self::ExpandedCase => "expanded-case",
            Self::CaseManifest => "case-manifest",
            Self::RootSummary => "root-summary",
            Self::CaseSummary => "case-summary",
            Self::ComparisonReport => "comparison-report",
            Self::AnimationReport => "animation-report",
            Self::WarningScan => "warning-scan",
            Self::ParityReport => "parity-report",
            Self::EnvironmentReport => "environment-report",
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ScenarioOverview {
    pub(crate) scenario_id: String,
    pub(crate) scenario_type: String,
    pub(crate) case_count: usize,
    #[serde(skip_serializing)]
    pub(crate) case_ids: Vec<String>,
    pub(crate) readiness_predicates: Vec<String>,
    pub(crate) pixel_anchor_count: usize,
    pub(crate) relative_pixel_anchor_count: usize,
    pub(crate) animation_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExpandedCaseOverview {
    pub(crate) schema_version: u64,
    pub(crate) case_id: String,
    pub(crate) manifest: Value,
    pub(crate) size: Value,
    pub(crate) color_scheme: String,
    pub(crate) artifact_dir: String,
    #[serde(default)]
    pub(crate) word_wrap: Option<bool>,
    #[serde(default)]
    pub(crate) direction: Option<String>,
    #[serde(default)]
    pub(crate) viewport_position: Option<String>,
    #[serde(default)]
    pub(crate) fixture_kind: Option<String>,
}

impl ExpandedCaseOverview {
    pub(crate) fn fixture_kind(&self) -> &str {
        self.fixture_kind.as_deref().unwrap_or("plain-lines")
    }

    pub(crate) fn scenario_type(&self) -> Option<&str> {
        self.manifest.get("scenario_type").and_then(Value::as_str)
    }
}

impl ScenarioOverview {
    pub(crate) fn filtered_case_count(&self, filter: &str) -> usize {
        self.case_ids
            .iter()
            .filter(|case_id| case_id.contains(filter))
            .count()
    }
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
    let kind = detect_document_kind(value);
    let schema_version = match schema_version(value) {
        Ok(schema_version) => schema_version,
        Err(_) if kind.accepts_legacy_unversioned() => SUPPORTED_SCHEMA_VERSION,
        Err(error) => return Err(error),
    };
    if schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ValidationError::unsupported(schema_version));
    }

    match kind {
        DocumentKind::VisualScenario => validate_visual_scenario(value)?,
        DocumentKind::ExpandedCase => {
            let _case = validate_deserializes::<ExpandedCase>(value, kind)?;
        }
        DocumentKind::CaseManifest => {
            let _manifest = validate_deserializes::<CaseManifestArtifact>(value, kind)?;
        }
        DocumentKind::RootSummary => {
            let _summary = validate_deserializes::<RootSummary>(value, kind)?;
        }
        DocumentKind::CaseSummary => {
            let _summary = validate_case_summary(value, kind)?;
        }
        DocumentKind::ComparisonReport => {
            let _report = validate_comparison_report(value, kind)?;
        }
        DocumentKind::AnimationReport => {
            validate_animation_report(value, kind)?;
        }
        DocumentKind::WarningScan => {
            validate_warning_scan(value, kind)?;
        }
        DocumentKind::ParityReport => {
            validate_parity_report(value, kind)?;
        }
        DocumentKind::EnvironmentReport => {
            validate_environment_report(value, kind)?;
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

impl DocumentKind {
    fn accepts_legacy_unversioned(self) -> bool {
        matches!(
            self,
            Self::CaseSummary | Self::ComparisonReport | Self::WarningScan
        )
    }
}

pub(crate) fn visual_scenario_overview(value: &Value) -> Result<ScenarioOverview, ValidationError> {
    validate_visual_scenario(value)?;
    let manifest =
        validate_deserializes::<VisualScenarioManifest>(value, DocumentKind::VisualScenario)?;
    Ok(manifest.overview())
}

pub(crate) fn expanded_visual_cases(
    value: &Value,
) -> Result<Vec<ExpandedCaseOverview>, ValidationError> {
    validate_visual_scenario(value)?;
    let manifest =
        validate_deserializes::<VisualScenarioManifest>(value, DocumentKind::VisualScenario)?;
    manifest.expanded_cases(value)
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
    } else if value.get("case").is_some() && value.get("source_manifest").is_some() {
        DocumentKind::CaseManifest
    } else if value.get("case_count").is_some() && value.get("cases").is_some() {
        DocumentKind::RootSummary
    } else if value.get("comparison_report").is_some()
        || (value.get("scenario_id").is_some()
            && value.get("final_geometry").is_some()
            && value.get("status").is_some())
    {
        DocumentKind::CaseSummary
    } else if value.get("matches").is_some() && value.get("status").is_some() {
        DocumentKind::WarningScan
    } else if value.get("parity").is_some()
        || (value.get("python_status").is_some() && value.get("rust_status").is_some())
        || (value.get("compared").is_some() && value.get("mismatches").is_some())
    {
        DocumentKind::ParityReport
    } else if value.get("host_capabilities").is_some()
        || value.get("missing_capabilities").is_some() && value.get("runtime").is_some()
    {
        DocumentKind::EnvironmentReport
    } else if value.get("animation_frame_evidence").is_some()
        || value.get("frames").is_some()
        || value.get("capture_mode").is_some() && value.get("geometry_samples").is_some()
    {
        DocumentKind::AnimationReport
    } else if value.get("protected_regions").is_some()
        || value.get("regions").is_some()
        || value.get("pixel_anchor_evidence").is_some()
        || value.get("pixel_anchors").is_some()
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
    match manifest.scenario_type.as_str() {
        "minimap-sidebar" => {
            if manifest.matrix.word_wrap.is_empty() {
                return Err(ValidationError::malformed(
                    "minimap-sidebar matrix.word_wrap must not be empty",
                ));
            }
            if manifest.matrix.directions.is_empty() {
                return Err(ValidationError::malformed(
                    "minimap-sidebar matrix.directions must not be empty",
                ));
            }
        }
        "command-palette-overlay" | "open-popover" => {}
        other => {
            return Err(ValidationError::malformed(format!(
                "unsupported scenario_type {other}"
            )));
        }
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
    if envelope.command == "artifact-summary" {
        let data = validate_deserializes::<AutomationArtifactSummaryData>(
            &envelope.data,
            DocumentKind::ArtifactEnvelope,
        )?;
        if data.status.is_none() && data.visual_geometry_cases.is_empty() {
            return Err(ValidationError::malformed(
                "artifact-summary data requires status or visual_geometry_cases",
            ));
        }
    }
    Ok(())
}

fn validate_case_summary(
    value: &Value,
    kind: DocumentKind,
) -> Result<CaseSummary, ValidationError> {
    let summary = validate_deserializes::<CaseSummary>(value, kind)?;
    if summary.status.trim().is_empty() {
        return Err(ValidationError::malformed(
            "case summary status must not be empty",
        ));
    }
    if summary.case_id.is_none() && summary.scenario_id.is_none() {
        return Err(ValidationError::malformed(
            "case summary requires case_id or scenario_id",
        ));
    }
    Ok(summary)
}

fn validate_comparison_report(
    value: &Value,
    kind: DocumentKind,
) -> Result<ComparisonReport, ValidationError> {
    let report = validate_deserializes::<ComparisonReport>(value, kind)?;
    if report.status.trim().is_empty() {
        return Err(ValidationError::malformed(
            "comparison report status must not be empty",
        ));
    }
    Ok(report)
}

fn validate_animation_report(value: &Value, kind: DocumentKind) -> Result<(), ValidationError> {
    let report = validate_deserializes::<AnimationReport>(value, kind)?;
    if report.status.trim().is_empty() {
        return Err(ValidationError::malformed(
            "animation report status must not be empty",
        ));
    }
    if report
        .capture_mode
        .as_deref()
        .is_some_and(|capture_mode| capture_mode == "stream")
        && report.max_sample_skew_ms.is_none()
    {
        return Err(ValidationError::malformed(
            "stream animation report requires max_sample_skew_ms",
        ));
    }
    Ok(())
}

fn validate_warning_scan(value: &Value, kind: DocumentKind) -> Result<(), ValidationError> {
    let report = validate_deserializes::<WarningScanReport>(value, kind)?;
    if report.status.trim().is_empty() {
        return Err(ValidationError::malformed(
            "warning scan status must not be empty",
        ));
    }
    Ok(())
}

fn validate_parity_report(value: &Value, kind: DocumentKind) -> Result<(), ValidationError> {
    let report = validate_deserializes::<ParityReport>(value, kind)?;
    if report.status.trim().is_empty() {
        return Err(ValidationError::malformed(
            "parity report status must not be empty",
        ));
    }
    if report.compared == Some(0) {
        return Err(ValidationError::malformed(
            "parity report compared count must be positive",
        ));
    }
    if report.rust_status.is_none() && report.rust_engine.is_none() {
        return Err(ValidationError::malformed(
            "parity report requires rust_status or rust_engine",
        ));
    }
    Ok(())
}

fn validate_environment_report(value: &Value, kind: DocumentKind) -> Result<(), ValidationError> {
    let report = validate_deserializes::<EnvironmentReport>(value, kind)?;
    if report.status.trim().is_empty() {
        return Err(ValidationError::malformed(
            "environment report status must not be empty",
        ));
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
    capture_steps: Vec<CaptureStep>,
    #[serde(default)]
    animation_sampling: Option<AnimationSampling>,
}

impl VisualScenarioManifest {
    fn overview(&self) -> ScenarioOverview {
        let case_ids = self.expanded_case_ids();
        ScenarioOverview {
            scenario_id: self.scenario_id.clone(),
            scenario_type: self.scenario_type.clone(),
            case_count: case_ids.len(),
            case_ids,
            readiness_predicates: self.readiness_predicates.clone(),
            pixel_anchor_count: self.pixel_anchors.len(),
            relative_pixel_anchor_count: self.relative_pixel_anchors.len(),
            animation_enabled: self
                .animation_sampling
                .as_ref()
                .is_some_and(|animation| animation.enabled),
        }
    }

    fn expanded_case_ids(&self) -> Vec<String> {
        self.expanded_cases(&Value::Null)
            .map(|cases| cases.into_iter().map(|case| case.case_id).collect())
            .unwrap_or_default()
    }

    fn expanded_cases(
        &self,
        manifest_value: &Value,
    ) -> Result<Vec<ExpandedCaseOverview>, ValidationError> {
        match self.scenario_type.as_str() {
            "minimap-sidebar" => self.minimap_sidebar_cases(manifest_value),
            "command-palette-overlay" => self.command_palette_cases(manifest_value),
            "open-popover" => self.open_popover_cases(manifest_value),
            _ => Ok(Vec::new()),
        }
    }

    fn minimap_sidebar_cases(
        &self,
        manifest_value: &Value,
    ) -> Result<Vec<ExpandedCaseOverview>, ValidationError> {
        let viewport_positions = default_when_empty(&self.matrix.viewport_positions, "top");
        let fixture_kinds = default_when_empty(&self.matrix.fixture_kinds, "plain-lines");
        let mut cases = Vec::new();
        for size in &self.matrix.sizes {
            for color_scheme in &self.matrix.color_schemes {
                for word_wrap in &self.matrix.word_wrap {
                    for direction in &self.matrix.directions {
                        for viewport_position in &viewport_positions {
                            for fixture_kind in &fixture_kinds {
                                if self.matrix.excludes(
                                    size,
                                    color_scheme,
                                    *word_wrap,
                                    direction,
                                    viewport_position,
                                    fixture_kind,
                                ) {
                                    continue;
                                }
                                let mut suffix = String::new();
                                if viewport_position != "top" {
                                    suffix.push_str("--");
                                    suffix.push_str(viewport_position);
                                }
                                if fixture_kind != "plain-lines" {
                                    suffix.push_str("--");
                                    suffix.push_str(fixture_kind);
                                }
                                let case_id = format!(
                                    "{}--{}--{}--wrap-{}--{}{}",
                                    self.scenario_id,
                                    size.id,
                                    color_scheme,
                                    word_wrap,
                                    direction,
                                    suffix
                                );
                                cases.push(ExpandedCaseOverview {
                                    schema_version: SUPPORTED_SCHEMA_VERSION,
                                    case_id: case_id.clone(),
                                    manifest: manifest_value.clone(),
                                    size: serde_json::to_value(size).map_err(|error| {
                                        ValidationError::malformed(format!(
                                            "case size serialization failed: {error}"
                                        ))
                                    })?,
                                    color_scheme: color_scheme.clone(),
                                    artifact_dir: case_id,
                                    word_wrap: Some(*word_wrap),
                                    direction: Some(direction.clone()),
                                    viewport_position: Some(viewport_position.clone()),
                                    fixture_kind: Some(fixture_kind.clone()),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(cases)
    }

    fn command_palette_cases(
        &self,
        manifest_value: &Value,
    ) -> Result<Vec<ExpandedCaseOverview>, ValidationError> {
        let mut cases = Vec::new();
        for size in &self.matrix.sizes {
            for color_scheme in &self.matrix.color_schemes {
                let case_id = format!("{}--{}--{}", self.scenario_id, size.id, color_scheme);
                cases.push(ExpandedCaseOverview {
                    schema_version: SUPPORTED_SCHEMA_VERSION,
                    case_id: case_id.clone(),
                    manifest: manifest_value.clone(),
                    size: serde_json::to_value(size).map_err(|error| {
                        ValidationError::malformed(format!(
                            "case size serialization failed: {error}"
                        ))
                    })?,
                    color_scheme: color_scheme.clone(),
                    artifact_dir: case_id,
                    word_wrap: Some(false),
                    direction: Some("open".to_string()),
                    viewport_position: None,
                    fixture_kind: Some("plain-lines".to_string()),
                });
            }
        }
        Ok(cases)
    }

    fn open_popover_cases(
        &self,
        manifest_value: &Value,
    ) -> Result<Vec<ExpandedCaseOverview>, ValidationError> {
        let fixture_kinds = default_when_empty(&self.matrix.fixture_kinds, "dense");
        let mut cases = Vec::new();
        for size in &self.matrix.sizes {
            for color_scheme in &self.matrix.color_schemes {
                for fixture_kind in &fixture_kinds {
                    let case_id = format!(
                        "{}--{}--{}--{}",
                        self.scenario_id, size.id, color_scheme, fixture_kind
                    );
                    cases.push(ExpandedCaseOverview {
                        schema_version: SUPPORTED_SCHEMA_VERSION,
                        case_id: case_id.clone(),
                        manifest: manifest_value.clone(),
                        size: serde_json::to_value(size).map_err(|error| {
                            ValidationError::malformed(format!(
                                "case size serialization failed: {error}"
                            ))
                        })?,
                        color_scheme: color_scheme.clone(),
                        artifact_dir: case_id,
                        word_wrap: Some(false),
                        direction: Some("open".to_string()),
                        viewport_position: None,
                        fixture_kind: Some(fixture_kind.clone()),
                    });
                }
            }
        }
        Ok(cases)
    }
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

impl ScenarioMatrix {
    fn excludes(
        &self,
        size: &ViewportSize,
        color_scheme: &str,
        word_wrap: bool,
        direction: &str,
        viewport_position: &str,
        fixture_kind: &str,
    ) -> bool {
        self.exclude.iter().any(|rule| {
            rule.matches(
                size,
                color_scheme,
                word_wrap,
                direction,
                viewport_position,
                fixture_kind,
            )
        })
    }
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

impl MatrixExclusion {
    fn matches(
        &self,
        size: &ViewportSize,
        color_scheme: &str,
        word_wrap: bool,
        direction: &str,
        viewport_position: &str,
        fixture_kind: &str,
    ) -> bool {
        if self.size.as_deref().is_some_and(|value| value != size.id) {
            return false;
        }
        if self
            .color_scheme
            .as_deref()
            .is_some_and(|value| value != color_scheme)
        {
            return false;
        }
        if self.word_wrap.is_some_and(|value| value != word_wrap) {
            return false;
        }
        if self
            .direction
            .as_deref()
            .is_some_and(|value| value != direction)
        {
            return false;
        }
        if self
            .viewport_position
            .as_deref()
            .is_some_and(|value| value != viewport_position)
        {
            return false;
        }
        if self
            .fixture_kind
            .as_deref()
            .is_some_and(|value| value != fixture_kind)
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CaptureStep {
    name: String,
    #[serde(default)]
    readiness_predicates: Vec<String>,
    #[serde(default)]
    screenshot: Option<String>,
    #[serde(default)]
    geometry_snapshot: Option<String>,
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
struct CaseManifestArtifact {
    schema_version: u64,
    scenario_id: String,
    scenario_type: String,
    #[serde(default)]
    invariant_id: Option<String>,
    #[serde(default)]
    source_manifest: Option<String>,
    #[serde(default)]
    case: Option<Value>,
    #[serde(default)]
    same_session: Option<Value>,
    #[serde(default)]
    protected_regions: Vec<ProtectedRegion>,
    #[serde(default)]
    pixel_anchors: Vec<PixelAnchor>,
    #[serde(default)]
    relative_pixel_anchors: Vec<RelativePixelAnchor>,
    #[serde(default)]
    allowed_changing_regions: Vec<AllowedChangingRegion>,
    #[serde(default)]
    animation_sampling: Option<AnimationSampling>,
    #[serde(default)]
    screenshots: Value,
    #[serde(default)]
    geometry_snapshots: Value,
    #[serde(default)]
    warnings: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct RootSummary {
    schema_version: u64,
    status: String,
    case_count: u64,
    #[serde(default)]
    passed: u64,
    #[serde(default)]
    failed: u64,
    #[serde(default)]
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
    #[serde(default)]
    engine: Option<Value>,
    #[serde(default)]
    scenario_source: Option<Value>,
    #[serde(default)]
    artifact_root: Option<String>,
    #[serde(default)]
    parity: Option<Value>,
    #[serde(default)]
    missing_capabilities: Vec<String>,
    cases: Vec<CaseSummary>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CaseSummary {
    #[serde(default)]
    schema_version: Option<u64>,
    #[serde(default)]
    case_id: Option<String>,
    #[serde(default)]
    scenario_id: Option<String>,
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
    #[serde(default)]
    artifact_dir: Option<String>,
    #[serde(default)]
    manifest: Option<String>,
    #[serde(default)]
    comparison_report: Option<String>,
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
    #[serde(default)]
    schema_version: Option<u64>,
    status: String,
    #[serde(default)]
    protected_regions: Vec<Value>,
    #[serde(default)]
    regions: Vec<Value>,
    #[serde(default)]
    allowed_changing_regions: Vec<Value>,
    #[serde(default)]
    pixel_anchor_evidence: Vec<Value>,
    #[serde(default)]
    pixel_anchors: Value,
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
    #[serde(default)]
    capture_mode: Option<String>,
    #[serde(default)]
    stream_frame_count: Option<u64>,
    #[serde(default)]
    sampled_frame_count: Option<u64>,
    #[serde(default)]
    geometry_sample_count: Option<u64>,
    #[serde(default)]
    intermediate_geometry_sample_count: Option<u64>,
    #[serde(default)]
    mapped_intermediate_frame_count: Option<u64>,
    #[serde(default)]
    max_sample_skew_ms: Option<u64>,
    #[serde(default)]
    max_sample_skew_observed_ms: Option<u64>,
    #[serde(default)]
    max_row_drift: Option<i64>,
    #[serde(default)]
    failures: Vec<Value>,
    #[serde(default)]
    geometry_samples: Vec<Value>,
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
struct WarningScanReport {
    #[serde(default)]
    schema_version: Option<u64>,
    status: String,
    #[serde(default)]
    matches: Vec<Value>,
    #[serde(default)]
    warning_count: Option<u64>,
    #[serde(default)]
    unexpected_count: Option<u64>,
    #[serde(default)]
    log_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ParityReport {
    schema_version: u64,
    status: String,
    #[serde(default)]
    compared: Option<u64>,
    #[serde(default)]
    failed: Option<u64>,
    #[serde(default)]
    mismatches: Vec<Value>,
    #[serde(default)]
    rust_status: Option<String>,
    #[serde(default)]
    python_status: Option<String>,
    #[serde(default)]
    rust_engine: Option<Value>,
    #[serde(default)]
    python_oracle: Option<Value>,
    #[serde(default)]
    corpus: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct EnvironmentReport {
    schema_version: u64,
    status: String,
    #[serde(default)]
    host_capabilities: Vec<Value>,
    #[serde(default)]
    missing_capabilities: Vec<String>,
    #[serde(default)]
    runtime: Option<Value>,
}

fn default_when_empty(values: &[String], fallback: &str) -> Vec<String> {
    if values.is_empty() {
        vec![fallback.to_string()]
    } else {
        values.to_vec()
    }
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
struct AutomationArtifactSummaryData {
    #[serde(default)]
    scenario_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    manifest: Option<Value>,
    #[serde(default)]
    summary: Option<Value>,
    #[serde(default)]
    warning_scan: Option<Value>,
    #[serde(default)]
    comparison_report: Option<Value>,
    #[serde(default)]
    visual_geometry_cases: Vec<Value>,
    #[serde(default)]
    verified_invariant_ids: Vec<String>,
    #[serde(default)]
    pixel_verified_invariant_ids: Vec<String>,
    #[serde(default)]
    animation_verified_invariant_ids: Vec<String>,
    #[serde(default)]
    engine: Option<Value>,
    #[serde(default)]
    parity: Option<Value>,
    #[serde(default)]
    dbus_artifacts: Vec<String>,
    #[serde(default)]
    state_assertions: Vec<Value>,
    #[serde(default)]
    waits: Vec<Value>,
    #[serde(default)]
    actions: Vec<Value>,
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
    fn overview_expands_current_case_matrix() {
        let value: Value = serde_json::from_str(include_str!(
            "../../../scripts/visual-geometry-scenarios/minimap-sidebar-live-threshold.json"
        ))
        .expect("scenario json");

        let overview = visual_scenario_overview(&value).expect("scenario overview");

        assert_eq!(overview.scenario_id, "minimap-sidebar-live-threshold");
        assert_eq!(overview.case_count, 2);
        assert_eq!(overview.filtered_case_count("show"), 1);
        assert_eq!(overview.pixel_anchor_count, 1);
        assert!(overview.animation_enabled);
    }

    #[test]
    fn accepts_python_skip_summary_shape() {
        let value = serde_json::json!({
            "schema_version": 1,
            "status": "skipped",
            "skip_reason": "missing compositor",
            "case_count": 0,
            "cases": []
        });

        let outcome = validate_document(&value).expect("skip summary");

        assert_eq!(outcome.kind, DocumentKind::RootSummary);
    }

    #[test]
    fn accepts_current_case_manifest_artifact_shape() {
        let value = serde_json::json!({
            "schema_version": 1,
            "scenario_id": "minimap-sidebar-live-threshold--live-1822x1272--force-light--wrap-true--hide",
            "scenario_type": "minimap-sidebar",
            "source_manifest": "minimap-sidebar-live-threshold.json",
            "case": { "id": "minimap-sidebar-live-threshold--live-1822x1272--force-light--wrap-true--hide" },
            "same_session": { "required": true },
            "protected_regions": [],
            "pixel_anchors": [],
            "screenshots": { "before": "before.png", "after": "after.png" },
            "geometry_snapshots": { "before": "before.json", "after": "after.json" },
            "warnings": { "status": "passed" }
        });

        let outcome = validate_document(&value).expect("case manifest");

        assert_eq!(outcome.kind, DocumentKind::CaseManifest);
        assert_eq!(outcome.schema_version, SUPPORTED_SCHEMA_VERSION);
    }

    #[test]
    fn accepts_current_legacy_case_summary_shape() {
        let value = serde_json::json!({
            "scenario_id": "minimap-sidebar-live-threshold--live-1822x1272--force-light--wrap-true--hide",
            "status": "passed",
            "failure_status": null,
            "comparison_report": "comparisons/comparison-report.json",
            "invariant_id": "native-minimap-highlight-anchors",
            "pixel_verified_invariant_ids": ["native-minimap-highlight-anchors"],
            "animation_verified_invariant_ids": ["native-minimap-animation-highlight-anchors"],
            "animation_frame_sample_count": 19,
            "final_geometry": { "before": [], "after": [] },
            "pixel_anchor_evidence": [],
            "rendered_anchor_stability": [],
            "app_vs_rendered_disagreements": [],
            "animation_frame_evidence": { "status": "passed", "frames": [] }
        });

        let outcome = validate_document(&value).expect("case summary");

        assert_eq!(outcome.kind, DocumentKind::CaseSummary);
        assert_eq!(outcome.schema_version, SUPPORTED_SCHEMA_VERSION);
    }

    #[test]
    fn accepts_current_legacy_comparison_report_shape() {
        let value = serde_json::json!({
            "status": "passed",
            "regions": [{
                "name": "header-bar",
                "surface": "header-bar",
                "status": "passed",
                "diff_pixels": 0
            }],
            "pixel_anchors": {
                "status": "passed",
                "anchors": [{
                    "name": "minimap-native-viewport-top-edge",
                    "status": "passed",
                    "detector": "native-minimap-viewport-top-edge-row"
                }],
                "app_vs_rendered_disagreements": [],
                "relationships": []
            },
            "final_geometry": { "before": [], "after": [] },
            "invariant_id": "native-minimap-highlight-anchors"
        });

        let outcome = validate_document(&value).expect("comparison report");

        assert_eq!(outcome.kind, DocumentKind::ComparisonReport);
    }

    #[test]
    fn accepts_current_legacy_warning_scan_shape() {
        let value = serde_json::json!({
            "status": "passed",
            "matches": []
        });

        let outcome = validate_document(&value).expect("warning scan");

        assert_eq!(outcome.kind, DocumentKind::WarningScan);
    }

    #[test]
    fn accepts_current_animation_report_shape() {
        let value = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "capture_mode": "stream",
            "invariant_id": "native-minimap-animation-highlight-anchors",
            "stream_frame_count": 48,
            "sampled_frame_count": 19,
            "geometry_sample_count": 18,
            "intermediate_geometry_sample_count": 5,
            "mapped_intermediate_frame_count": 5,
            "max_sample_skew_ms": 80,
            "max_sample_skew_observed_ms": 12,
            "max_row_drift": 0,
            "frames": [{
                "frame_index": 0,
                "elapsed_ms": 65,
                "mapped_sample_elapsed_ms": 61,
                "sidebar_phase": "intermediate",
                "anchors": []
            }]
        });

        let outcome = validate_document(&value).expect("animation report");

        assert_eq!(outcome.kind, DocumentKind::AnimationReport);
    }

    #[test]
    fn rejects_malformed_animation_report() {
        let value = serde_json::json!({
            "schema_version": 1,
            "status": "failed",
            "capture_mode": "stream",
            "frames": []
        });

        let error = validate_document(&value).expect_err("malformed animation");

        assert_eq!(error.status, ProofStatus::MalformedField);
        assert!(error.detail.contains("max_sample_skew_ms"));
    }

    #[test]
    fn accepts_parity_report_metadata() {
        let value = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "compared": 12,
            "failed": 0,
            "mismatches": [],
            "rust_engine": { "name": "cargo-gtk-proof" },
            "python_oracle": { "name": "visual-geometry-smoke.py" }
        });

        let outcome = validate_document(&value).expect("parity report");

        assert_eq!(outcome.kind, DocumentKind::ParityReport);
    }

    #[test]
    fn accepts_automation_artifact_summary_envelope_data() {
        let value = serde_json::json!({
            "ok": true,
            "status": "ok",
            "command": "artifact-summary",
            "detail": "generic smoke artifact summary read",
            "version": { "schema_version": 1, "tool_version": "test" },
            "data": {
                "scenario_id": "minimap-sidebar-live-threshold",
                "status": "passed",
                "visual_geometry_cases": [{
                    "case_id": "minimap-sidebar-live-threshold--wide--force-light--wrap-true--hide",
                    "status": "passed"
                }],
                "verified_invariant_ids": ["native-minimap-highlight-anchors"],
                "pixel_verified_invariant_ids": ["native-minimap-highlight-anchors"],
                "animation_verified_invariant_ids": ["native-minimap-animation-highlight-anchors"],
                "engine": { "name": "cargo-gtk-proof" },
                "parity": { "status": "passed" },
                "dbus_artifacts": [],
                "state_assertions": [],
                "waits": [],
                "actions": []
            }
        });

        let outcome = validate_document(&value).expect("artifact-summary envelope");

        assert_eq!(outcome.kind, DocumentKind::ArtifactEnvelope);
    }

    #[test]
    fn rejects_malformed_automation_artifact_summary_data() {
        let value = serde_json::json!({
            "ok": true,
            "status": "ok",
            "command": "artifact-summary",
            "detail": "generic smoke artifact summary read",
            "version": { "schema_version": 1 },
            "data": {}
        });

        let error = validate_document(&value).expect_err("malformed artifact summary data");

        assert_eq!(error.status, ProofStatus::MalformedField);
        assert!(error.detail.contains("artifact-summary data"));
    }

    #[test]
    fn rejects_malformed_parity_metadata() {
        let value = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "compared": 0,
            "failed": 0,
            "mismatches": []
        });

        let error = validate_document(&value).expect_err("malformed parity");

        assert_eq!(error.status, ProofStatus::MalformedField);
        assert!(error.detail.contains("compared count"));
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
