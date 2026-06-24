// SPDX-License-Identifier: GPL-3.0-or-later

//! Adoption-lab pages for proof-harness and GTK-free proof-spine contracts.

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_proof_harness::{HarnessConfig, RegisteredTest, recommended_pre_gtk_environment};
use gtk_lush_proof_spine::{
    ArtifactEnvelope, ArtifactSummaryProvider, BlockerSummary, PrivacyScope, ProofStatus,
    ReadinessPredicate, ReadinessProvider, ReadinessResult, Rect, SnapshotEnvelope,
    SnapshotProvider, SurfaceSummary, VersionInfo, WorkflowEvent, WorkflowEventProvider,
    WorkflowPhase,
};

use crate::shared_ui::{append_body, append_fact, scroll_page, workflow_box};

/// Keeps proof-harness configuration alive for shell lifecycle reporting.
pub(crate) struct ProofHarnessOwners {
    config: HarnessConfig,
}

impl ProofHarnessOwners {
    /// Return the harness environment pair shown in the shell lifecycle summary.
    pub(crate) fn attempt_summary(&self) -> String {
        format!(
            "{}:{}",
            self.config.child_test_env(),
            self.config.headless_runner_env()
        )
    }
}

/// Build the page that demonstrates headless proof-harness configuration.
pub(crate) fn build_proof_harness_page() -> (gtk4::Widget, ProofHarnessOwners) {
    let content = workflow_box("Headless Proof Harness Contract");
    append_body(
        &content,
        "The lab keeps test registration, waits, and child-process environment \
         settings outside LushText resources.",
    );

    let config = adoption_harness_config();
    let registered = RegisteredTest::new("adoption-lab-smoke", harness_smoke_test);
    let env_summary = recommended_pre_gtk_environment()
        .into_iter()
        .map(|entry| format!("{}={}", entry.key, entry.value))
        .collect::<Vec<_>>()
        .join(", ");

    append_fact(&content, "child env", config.child_test_env());
    append_fact(&content, "runner env", config.headless_runner_env());
    append_fact(&content, "monitor env", config.headless_monitor_env());
    append_fact(&content, "registered test", registered.name());
    append_fact(&content, "recommended env", &env_summary);

    (scroll_page(&content), ProofHarnessOwners { config })
}

fn adoption_harness_config() -> HarnessConfig {
    HarnessConfig::new(
        "GTK_LUSH_ADOPTION_LAB_CHILD_TEST",
        "GTK_LUSH_ADOPTION_LAB_HEADLESS",
        "GTK_LUSH_ADOPTION_LAB_MONITOR",
    )
    .with_default_headless_monitor("1280x900")
    .with_test_attempts(1)
    .with_runner_label("GTK Lush adoption lab")
}

fn harness_smoke_test() {}

#[derive(Clone)]
struct DemoProofProvider {
    sequence: Rc<Cell<u64>>,
}

impl DemoProofProvider {
    fn new() -> Self {
        Self {
            sequence: Rc::new(Cell::new(1)),
        }
    }
}

impl ReadinessProvider for DemoProofProvider {
    fn readiness(&self, predicate: &ReadinessPredicate) -> ReadinessResult {
        match predicate.as_str() {
            "lab-idle" => ReadinessResult::ready(predicate.clone()),
            "render-hold-settled" => ReadinessResult::blocked(
                predicate.clone(),
                BlockerSummary::new("render-hold").with_detail("cover is warming"),
            ),
            _ => ReadinessResult::unknown(predicate.clone()),
        }
    }
}

impl SnapshotProvider for DemoProofProvider {
    fn snapshot(&self) -> SnapshotEnvelope {
        let sequence = self.sequence.get();
        SnapshotEnvelope {
            version: VersionInfo::current().with_interface_version("adoption-lab"),
            surfaces: vec![
                SurfaceSummary::new("signals", true).with_rect(Rect::new(0, 0, 280, 180)),
                SurfaceSummary::new("render-hold", true).with_rect(Rect::new(300, 0, 360, 180)),
            ],
            workflows: self.workflow_events(),
            privacy: PrivacyScope::PublicDiagnostic,
            ..SnapshotEnvelope::new(sequence)
        }
    }
}

impl WorkflowEventProvider for DemoProofProvider {
    fn workflow_events(&self) -> Vec<WorkflowEvent> {
        vec![
            WorkflowEvent {
                workflow_id: "adoption-lab".to_string(),
                phase: WorkflowPhase::Start,
                status: ProofStatus::Ready,
                sequence: self.sequence.get(),
                detail: Some("bounded workflow metadata only".to_string()),
                blocker: None,
            },
            WorkflowEvent {
                workflow_id: "render-hold".to_string(),
                phase: WorkflowPhase::Progress,
                status: ProofStatus::Blocked,
                sequence: self.sequence.get().saturating_add(1),
                detail: Some("cover warming".to_string()),
                blocker: Some(BlockerSummary::new("render-hold")),
            },
        ]
    }
}

impl ArtifactSummaryProvider for DemoProofProvider {
    fn summarize_artifact(&self, command: &str) -> ArtifactEnvelope {
        ArtifactEnvelope::success(command, "adoption lab summary").with_data(serde_json::json!({
            "workflow_count": 7,
            "privacy": "public-diagnostic"
        }))
    }
}

/// Keeps the demo proof provider alive for shell lifecycle reporting.
pub(crate) struct ProofSpineOwners {
    provider: DemoProofProvider,
}

impl ProofSpineOwners {
    /// Return the current bounded snapshot sequence from the demo provider.
    pub(crate) fn snapshot_sequence(&self) -> u64 {
        self.provider.snapshot().sequence
    }
}

/// Build the page that demonstrates GTK-free proof-spine value objects.
pub(crate) fn build_proof_spine_page() -> (gtk4::Widget, ProofSpineOwners) {
    let content = workflow_box("GTK-Free Proof Spine Values");
    append_body(
        &content,
        "The provider owns application state and only emits bounded readiness, \
         workflow, snapshot, and artifact values.",
    );

    let provider = DemoProofProvider::new();
    let idle = provider.readiness(&ReadinessPredicate::new("lab-idle"));
    let blocked = provider.readiness(&ReadinessPredicate::new("render-hold-settled"));
    let snapshot = provider.snapshot();
    let artifact = provider.summarize_artifact("lab-summary");

    append_fact(&content, "idle ready", &idle.ready.to_string());
    append_fact(&content, "blocked status", &format!("{:?}", blocked.status));
    append_fact(
        &content,
        "snapshot surfaces",
        &snapshot.surfaces.len().to_string(),
    );
    append_fact(
        &content,
        "workflow events",
        &snapshot.workflows.len().to_string(),
    );
    append_fact(&content, "artifact ok", &artifact.ok.to_string());

    (scroll_page(&content), ProofSpineOwners { provider })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_harness_config_uses_lab_environment_names() {
        let config = adoption_harness_config();

        assert_eq!(config.child_test_env(), "GTK_LUSH_ADOPTION_LAB_CHILD_TEST");
        assert_eq!(
            config.headless_runner_env(),
            "GTK_LUSH_ADOPTION_LAB_HEADLESS"
        );
        assert_eq!(recommended_pre_gtk_environment().len(), 4);
    }

    #[test]
    fn proof_provider_emits_bounded_snapshot_and_artifact() {
        let provider = DemoProofProvider::new();
        let ready = provider.readiness(&ReadinessPredicate::new("lab-idle"));
        let blocked = provider.readiness(&ReadinessPredicate::new("render-hold-settled"));
        let snapshot = provider.snapshot();
        let artifact = provider.summarize_artifact("lab-summary");

        assert!(ready.ready);
        assert_eq!(blocked.status, ProofStatus::Blocked);
        assert_eq!(snapshot.privacy, PrivacyScope::PublicDiagnostic);
        assert_eq!(snapshot.surfaces.len(), 2);
        assert!(artifact.ok);
    }
}
