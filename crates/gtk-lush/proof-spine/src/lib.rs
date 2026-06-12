// SPDX-License-Identifier: MIT OR Apache-2.0

//! GTK-free proof protocol primitives for gtk-rs applications.
//!
//! This crate provides value objects and traits for readiness predicates,
//! blockers, workflow events, bounded snapshots, visual surface summaries, and
//! artifact result envelopes. Applications adapt these values to their own
//! D-Bus, CLI, or test transport; the crate deliberately does not own command
//! dispatch, GTK actions, widget trees, or application state.
//!
//! GTK Lush crates remain independently adoptable leaf crates. They do not own
//! GTK control flow, define a view DSL, add a state/message framework, depend
//! on another GTK Lush crate, or replace Libadwaita adaptive behavior.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// Current schema version for proof-spine JSON envelopes.
///
/// Version `1` is the first in-tree schema used by the Phase 4 extraction. New
/// required fields must bump this value; additive optional fields can keep it.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Version metadata carried by proof snapshots and artifact envelopes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VersionInfo {
    /// Machine-readable schema version for the serialized envelope.
    pub schema_version: u32,
    /// Optional application or interface version that produced the envelope.
    pub interface_version: Option<String>,
    /// Optional tool version that produced the envelope.
    pub tool_version: Option<String>,
}

impl VersionInfo {
    /// Create version metadata for the current proof-spine schema.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            interface_version: None,
            tool_version: None,
        }
    }

    /// Attach an application or interface version.
    #[must_use]
    pub fn with_interface_version(mut self, version: impl Into<String>) -> Self {
        self.interface_version = Some(version.into());
        self
    }

    /// Attach a proof-tool version.
    #[must_use]
    pub fn with_tool_version(mut self, version: impl Into<String>) -> Self {
        self.tool_version = Some(version.into());
        self
    }
}

impl Default for VersionInfo {
    fn default() -> Self {
        Self::current()
    }
}

/// Stable status vocabulary shared by readiness, snapshots, and artifacts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofStatus {
    /// The requested predicate or proof is ready.
    Ready,
    /// The requested predicate or proof passed.
    Passed,
    /// The requested predicate or proof failed.
    Failed,
    /// Readiness is blocked by known in-progress work.
    Blocked,
    /// The requested predicate did not become ready before timeout.
    PredicateTimeout,
    /// The provider does not support the requested predicate.
    UnknownPredicate,
    /// The host lacks required compositor, D-Bus, screenshot, or tool support.
    UnsupportedHost,
    /// The proof was intentionally skipped and must not count as verified.
    Skipped,
    /// The application reported a bounded workflow or collection failure.
    ApplicationFailure,
    /// The caller provided invalid input or arguments.
    UsageError,
    /// The artifact declares a schema version this tool does not support.
    UnsupportedSchemaVersion,
    /// The artifact is syntactically valid but missing required fields.
    MalformedField,
    /// Proof policy rejected the available artifact evidence.
    PolicyFailure,
    /// The artifact shape was missing, malformed, or incompatible.
    ArtifactError,
}

/// Identifier for a readiness predicate understood by an application provider.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ReadinessPredicate(String);

impl ReadinessPredicate {
    /// Create a readiness predicate identifier.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Return the predicate as a string slice for transport adapters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ReadinessPredicate {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Bounded explanation for why readiness or proof is blocked.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockerSummary {
    /// Workflow or surface class causing the block.
    pub kind: String,
    /// Bounded human-readable detail safe for logs and CI artifacts.
    pub detail: Option<String>,
}

impl BlockerSummary {
    /// Create a blocker summary with no extra detail.
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            detail: None,
        }
    }

    /// Attach bounded human-readable detail to the blocker.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Readiness result for one predicate evaluation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadinessResult {
    /// Predicate that was evaluated.
    pub predicate: ReadinessPredicate,
    /// Stable status for this predicate evaluation.
    pub status: ProofStatus,
    /// Whether the predicate is ready and can be treated as satisfied.
    pub ready: bool,
    /// Optional bounded blocker when `ready` is false.
    pub blocker: Option<BlockerSummary>,
    /// Optional bounded detail safe for logs and artifacts.
    pub detail: Option<String>,
}

impl ReadinessResult {
    /// Build a successful readiness result.
    #[must_use]
    pub fn ready(predicate: impl Into<ReadinessPredicate>) -> Self {
        Self {
            predicate: predicate.into(),
            status: ProofStatus::Ready,
            ready: true,
            blocker: None,
            detail: None,
        }
    }

    /// Build a blocked readiness result.
    #[must_use]
    pub fn blocked(predicate: impl Into<ReadinessPredicate>, blocker: BlockerSummary) -> Self {
        Self {
            predicate: predicate.into(),
            status: ProofStatus::Blocked,
            ready: false,
            blocker: Some(blocker),
            detail: None,
        }
    }

    /// Build an unknown-predicate readiness result.
    #[must_use]
    pub fn unknown(predicate: impl Into<ReadinessPredicate>) -> Self {
        Self {
            predicate: predicate.into(),
            status: ProofStatus::UnknownPredicate,
            ready: false,
            blocker: None,
            detail: Some("predicate is not supported by this provider".to_string()),
        }
    }
}

/// Privacy classification for a snapshot or artifact field set.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrivacyScope {
    /// The value is safe for CI logs and agent-readable summaries.
    PublicDiagnostic,
    /// The value is safe only as a bounded artifact path or count.
    BoundedArtifact,
    /// The value was redacted or intentionally omitted by the app adapter.
    Redacted,
}

/// Integer rectangle used by visual geometry snapshots.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Rect {
    /// Left coordinate in logical pixels.
    pub x: i32,
    /// Top coordinate in logical pixels.
    pub y: i32,
    /// Width in logical pixels.
    pub width: i32,
    /// Height in logical pixels.
    pub height: i32,
}

impl Rect {
    /// Create a rectangle from logical-pixel coordinates.
    #[must_use]
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Bounded visual surface summary for snapshots and proof artifacts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceSummary {
    /// Stable surface name, such as `editor`, `sidebar`, or `minimap`.
    pub name: String,
    /// Whether the surface is currently visible according to the app adapter.
    pub visible: bool,
    /// Optional logical rectangle for the surface.
    pub rect: Option<Rect>,
    /// Optional scale factor associated with the captured surface.
    pub scale_factor: Option<i32>,
}

impl SurfaceSummary {
    /// Create a named surface summary.
    #[must_use]
    pub fn new(name: impl Into<String>, visible: bool) -> Self {
        Self {
            name: name.into(),
            visible,
            rect: None,
            scale_factor: None,
        }
    }

    /// Attach a logical rectangle to this surface.
    #[must_use]
    pub const fn with_rect(mut self, rect: Rect) -> Self {
        self.rect = Some(rect);
        self
    }
}

/// Phase of an app-owned workflow event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowPhase {
    /// Workflow started.
    Start,
    /// Workflow emitted bounded progress.
    Progress,
    /// Workflow finished successfully.
    Finish,
    /// Workflow skipped because the requested support was unavailable.
    Skip,
    /// Workflow failed with a bounded diagnostic.
    Failure,
}

/// Bounded event emitted by an app-owned workflow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowEvent {
    /// Stable workflow identity chosen by the provider.
    pub workflow_id: String,
    /// Phase represented by this event.
    pub phase: WorkflowPhase,
    /// Stable status for the event.
    pub status: ProofStatus,
    /// Monotonic sequence number or timestamp-like ordering key.
    pub sequence: u64,
    /// Optional bounded detail safe for logs and artifacts.
    pub detail: Option<String>,
    /// Optional blocker associated with this event.
    pub blocker: Option<BlockerSummary>,
}

impl WorkflowEvent {
    /// Create a workflow event with no detail or blocker.
    #[must_use]
    pub fn new(
        workflow_id: impl Into<String>,
        phase: WorkflowPhase,
        status: ProofStatus,
        sequence: u64,
    ) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            phase,
            status,
            sequence,
            detail: None,
            blocker: None,
        }
    }
}

/// Bounded snapshot envelope produced by an app-owned provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotEnvelope {
    /// Version metadata for this snapshot.
    pub version: VersionInfo,
    /// Stable status describing whether the snapshot is usable.
    pub status: ProofStatus,
    /// Monotonic capture sequence number when available.
    pub sequence: u64,
    /// Bounded visual surfaces included in the snapshot.
    pub surfaces: Vec<SurfaceSummary>,
    /// Recent bounded workflow events associated with the snapshot.
    pub workflows: Vec<WorkflowEvent>,
    /// Privacy classification for this envelope.
    pub privacy: PrivacyScope,
}

impl SnapshotEnvelope {
    /// Create an empty successful snapshot envelope.
    #[must_use]
    pub fn new(sequence: u64) -> Self {
        Self {
            version: VersionInfo::current(),
            status: ProofStatus::Ready,
            sequence,
            surfaces: Vec::new(),
            workflows: Vec::new(),
            privacy: PrivacyScope::PublicDiagnostic,
        }
    }
}

/// Stable result envelope for CLI and artifact-summary commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactEnvelope {
    /// Whether the command or summary succeeded.
    pub ok: bool,
    /// Stable machine-readable status.
    pub status: ProofStatus,
    /// Command, scenario, or artifact identity.
    pub command: String,
    /// Bounded human-readable detail.
    pub detail: String,
    /// Version metadata for the envelope.
    pub version: VersionInfo,
    /// Safe command-specific data.
    pub data: serde_json::Value,
}

impl ArtifactEnvelope {
    /// Create a successful envelope with empty data.
    #[must_use]
    pub fn success(command: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            ok: true,
            status: ProofStatus::Passed,
            command: command.into(),
            detail: detail.into(),
            version: VersionInfo::current(),
            data: serde_json::json!({}),
        }
    }

    /// Create a non-success envelope with empty data.
    #[must_use]
    pub fn failure(
        command: impl Into<String>,
        status: ProofStatus,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            status,
            command: command.into(),
            detail: detail.into(),
            version: VersionInfo::current(),
            data: serde_json::json!({}),
        }
    }

    /// Attach safe JSON data to the envelope.
    #[must_use]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
}

/// App-owned provider for readiness predicates.
pub trait ReadinessProvider {
    /// Evaluate one readiness predicate without blocking the GTK main loop.
    fn readiness(&self, predicate: &ReadinessPredicate) -> ReadinessResult;
}

/// App-owned provider for bounded snapshots.
pub trait SnapshotProvider {
    /// Collect a bounded snapshot envelope from application state.
    fn snapshot(&self) -> SnapshotEnvelope;
}

/// App-owned provider for recent workflow events.
pub trait WorkflowEventProvider {
    /// Return bounded workflow events in provider-defined order.
    fn workflow_events(&self) -> Vec<WorkflowEvent>;
}

/// App-owned provider for proof artifact summaries.
pub trait ArtifactSummaryProvider {
    /// Summarize a proof artifact through the stable result envelope.
    fn summarize_artifact(&self, command: &str) -> ArtifactEnvelope;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_round_trips_through_json() {
        let result =
            ReadinessResult::blocked("visual-geometry-settled", BlockerSummary::new("layout"));

        let encoded = serde_json::to_string(&result).expect("serialize readiness");
        let decoded: ReadinessResult =
            serde_json::from_str(&encoded).expect("deserialize readiness");

        assert_eq!(decoded, result);
        assert_eq!(decoded.status, ProofStatus::Blocked);
    }

    #[test]
    fn snapshot_carries_bounded_visual_surface() {
        let snapshot = SnapshotEnvelope {
            surfaces: vec![
                SurfaceSummary::new("minimap", true).with_rect(Rect::new(10, 20, 30, 40)),
            ],
            ..SnapshotEnvelope::new(7)
        };

        let encoded = serde_json::to_value(&snapshot).expect("snapshot json");

        assert_eq!(encoded["sequence"], 7);
        assert_eq!(encoded["surfaces"][0]["name"], "minimap");
        assert!(encoded["surfaces"][0]["visible"].as_bool().unwrap_or(false));
    }

    #[test]
    fn artifact_envelope_distinguishes_skip_from_success() {
        let envelope =
            ArtifactEnvelope::failure("run", ProofStatus::UnsupportedHost, "missing mutter");

        assert!(!envelope.ok);
        assert_eq!(envelope.status, ProofStatus::UnsupportedHost);
        assert_eq!(envelope.command, "run");
    }
}
