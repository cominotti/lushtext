// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::services::filesystem::write_protocol::{StepOutcome, WriteAction};
use crate::services::filesystem::{WriteLabel, fixture, write as fs_write};

/// The `sys.rs` primitives that are fallible but deliberately make no tap
/// call, each with its reason. Any other fallible primitive without a tap
/// call fails [`every_fallible_backend_primitive_reports_to_the_tap`].
const UNTAPPED_PRIMITIVES: [(&str, &str); 3] = [
    (
        "open_append_create",
        "measurement evidence only (responsiveness probes), never a durable or draft path",
    ),
    (
        "create_new_empty_file",
        "a composite of create_temp_file and sync_file, which report themselves",
    ),
    (
        "path_exists",
        "a composite of metadata, which reports itself",
    ),
];

/// The names and bodies of the fallible `pub(in crate::services)` functions
/// of `sys.rs` outside its fixture module, skipping non-Unix variants.
fn fallible_primitives(source: &str) -> Vec<(String, String)> {
    let production = &source[..source
        .find("mod fixture_ops")
        .expect("sys.rs keeps its fixture module")];
    let mut found = Vec::new();
    for (start, _) in production.match_indices("pub(in crate::services) fn ") {
        let preceding = production[..start]
            .rsplit_once("\n\n")
            .map_or("", |(_, attributes)| attributes);
        if preceding.contains("#[cfg(not(unix))]") {
            continue;
        }
        let signature_end = start + production[start..].find('{').expect("a body");
        let signature = &production[start..signature_end];
        if !signature.contains("io::Result") && !signature.contains("Result<") {
            continue;
        }
        let name = signature["pub(in crate::services) fn ".len()..]
            .split(['(', '<'])
            .next()
            .expect("a name")
            .to_string();
        let mut depth = 0usize;
        let mut end = signature_end;
        for (offset, byte) in production.as_bytes()[signature_end..].iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = signature_end + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push((name, production[signature_end..end].to_string()));
    }
    found
}

/// Every fallible backend primitive the durable-write shells and the draft
/// service can reach reports to the tap, directly or through the reader and
/// directory openers, so a primitive added later without a tap call fails
/// here instead of being invisible to the conformance tests.
#[test]
fn every_fallible_backend_primitive_reports_to_the_tap() {
    let primitives = fallible_primitives(include_str!("../sys.rs"));
    assert!(primitives.len() >= 20, "the scan found the primitives");
    let untapped: Vec<&str> = primitives
        .iter()
        .filter(|(name, body)| {
            !(body.contains("tap::call(")
                || body.contains("open_for_read(")
                || body.contains("open_directory(")
                || UNTAPPED_PRIMITIVES.iter().any(|(exempt, _)| exempt == name))
        })
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        untapped,
        Vec::<&str>::new(),
        "primitives without a tap call"
    );
}

#[test]
fn the_pin_finds_a_primitive_without_a_tap_call() {
    let source = "pub(in crate::services) fn read(path: &Path) -> io::Result<Vec<u8>> {\n    fs::read(path)\n}\n\nmod fixture_ops {}\n";
    assert_eq!(
        fallible_primitives(source),
        vec![("read".to_string(), "{\n    fs::read(path)\n".to_string())]
    );
}

#[test]
fn a_recorded_write_lists_each_action_its_outcome_and_its_calls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data.txt");
    fixture::write_text(&path, "old");
    let guard = install(TapPlan::record());
    fs_write::atomic_replace(&path, WriteLabel::SAVE, b"new").expect("write");
    let trace = guard.finish();
    assert_eq!(trace.runs.len(), 1);
    let actions: Vec<WriteAction> = trace
        .steps
        .iter()
        .map(|step| step.action().expect("a write action"))
        .collect();
    assert_eq!(
        actions,
        vec![
            WriteAction::ProbeMetadata,
            WriteAction::CreateTemp,
            WriteAction::WriteContent,
            WriteAction::ApplyMetadata,
            WriteAction::SyncTemp,
            WriteAction::Rename,
            WriteAction::SyncDir,
        ]
    );
    assert!(
        trace
            .steps
            .iter()
            .all(|step| step.outcome::<StepOutcome>() == Some(StepOutcome::Done))
    );
    let rename = &trace.steps[5];
    assert_eq!(rename.calls.len(), 1);
    assert_eq!(rename.calls[0].call, Call::Rename);
    assert_eq!(rename.calls[0].path, path);
}

#[test]
fn an_injected_collision_takes_the_next_temp_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data.txt");
    let guard = install(TapPlan::record().inject(
        Address::Protocol {
            kind: ProtocolKind::Write,
            run: 0,
            ordinal: 1,
            call: 0,
        },
        Injected::ALREADY_EXISTS,
    ));
    fs_write::atomic_replace(&path, WriteLabel::SAVE, b"new").expect("write");
    let trace = guard.finish();
    assert!(trace.unfired.is_empty());
    assert_eq!(
        trace.steps[1].outcome::<StepOutcome>(),
        Some(StepOutcome::AlreadyExists)
    );
    assert_eq!(
        trace.steps[2].action::<WriteAction>(),
        Some(WriteAction::CreateTemp)
    );
    fixture::assert_text(&path, "new");
}

#[test]
fn a_crash_before_the_rename_leaves_the_temp_and_the_previous_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data.txt");
    fixture::write_text(&path, "old");
    let guard = install(TapPlan::record().crash(CrashAt {
        at: ActionAt::Named {
            kind: ProtocolKind::Write,
            name: "Rename",
            nth: 0,
        },
        after: false,
    }));
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fs_write::atomic_replace(&path, WriteLabel::SAVE, b"new")
    }))
    .expect_err("the tap crashed the write");
    assert!(is_tap_crash(unwound.as_ref()));
    drop(guard);
    fixture::assert_text(&path, "old");
    assert_eq!(
        fixture::entry_names(dir.path()).len(),
        2,
        "the temp file is left as a crash leaves it"
    );
}

#[test]
fn a_scope_wide_address_fails_the_first_tap_point_of_any_primitive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data.txt");
    fixture::write_text(&path, "old");
    let guard = install(TapPlan::record().inject(Address::ScopeWide { call: 0 }, Injected::FAIL));
    let result = fs_write::atomic_replace(&path, WriteLabel::SAVE, b"new");
    let trace = guard.finish();
    assert!(trace.unfired.is_empty());
    assert!(
        matches!(result, Err(fs_write::DurableWriteError::BeforeRename(_))),
        "{result:?}"
    );
    assert_eq!(
        trace.steps[0].action::<WriteAction>(),
        Some(WriteAction::ProbeMetadata)
    );
    fixture::assert_text(&path, "old");
}

#[test]
fn a_destination_address_fails_the_directory_sync_after_that_rename() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data.txt");
    let other = dir.path().join("other.txt");
    let guard = install(TapPlan::record().inject(
        Address::Destination {
            kind: ProtocolKind::Write,
            destination: path.clone(),
            action: "SyncDir",
            call: 0,
        },
        Injected::FAIL,
    ));
    // A write to another destination is not addressed.
    fs_write::atomic_replace(&other, WriteLabel::SAVE, b"other").expect("other write");
    let result = fs_write::atomic_replace(&path, WriteLabel::SAVE, b"new");
    let trace = guard.finish();
    assert!(trace.unfired.is_empty());
    assert!(
        matches!(result, Err(fs_write::DurableWriteError::AfterRename(_))),
        "{result:?}"
    );
    fixture::assert_text(&path, "new");
    fixture::assert_text(&other, "other");
}
