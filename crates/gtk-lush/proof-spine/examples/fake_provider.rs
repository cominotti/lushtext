// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fake automation provider for the GTK Lush proof-spine contracts.
//!
//! The example keeps readiness, snapshot, event, and artifact providers small
//! enough to copy while still exercising the adoption-facing trait surface.

use gtk_lush_proof_spine::{
    ArtifactEnvelope, ArtifactSummaryProvider, ReadinessPredicate, ReadinessProvider,
    ReadinessResult, SnapshotEnvelope, SnapshotProvider, SurfaceSummary, WorkflowEvent,
    WorkflowEventProvider,
};

struct FakeApp {
    ready: bool,
}

impl ReadinessProvider for FakeApp {
    fn readiness(&self, predicate: &ReadinessPredicate) -> ReadinessResult {
        if predicate.as_str() == "idle" && self.ready {
            ReadinessResult::ready("idle")
        } else {
            ReadinessResult::unknown(predicate.as_str())
        }
    }
}

impl SnapshotProvider for FakeApp {
    fn snapshot(&self) -> SnapshotEnvelope {
        let mut snapshot = SnapshotEnvelope::new(1);
        snapshot
            .surfaces
            .push(SurfaceSummary::new("example-window", true));
        snapshot
    }
}

impl WorkflowEventProvider for FakeApp {
    fn workflow_events(&self) -> Vec<WorkflowEvent> {
        Vec::new()
    }
}

impl ArtifactSummaryProvider for FakeApp {
    fn summarize_artifact(&self, command: &str) -> ArtifactEnvelope {
        ArtifactEnvelope::success(command, "fake artifact passed")
    }
}

fn main() {
    let app = FakeApp { ready: true };
    let readiness = app.readiness(&ReadinessPredicate::new("idle"));
    let snapshot = app.snapshot();
    let summary = app.summarize_artifact("fake");

    println!(
        "{} {} {}",
        readiness.ready,
        snapshot.surfaces.len(),
        summary.ok
    );
}
