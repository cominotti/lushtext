// SPDX-License-Identifier: GPL-3.0-or-later

//! Visual proof policy checks for the Rust proof tool.
//!
//! The default Makefile wrapper still uses the Python policy checker in this
//! phase. Keeping the Rust mapping side-by-side lets the migration prove that
//! sensitive files, required invariants, and negative animation cases keep the
//! same meaning before wrappers move.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::read_json_value;

const VISUAL_SENSITIVE_PREFIXES: &[&str] = &[
    "crates/lushtext-core/src/ui/",
    "crates/lushtext/tests/widget/",
    "resources/ui/",
    "resources/style/",
    "scripts/visual-geometry-scenarios/",
];
const VISUAL_SENSITIVE_EXACT: &[&str] = &[
    "crates/lushtext-core/src/model/automation.rs",
    "scripts/check-visual-proof-policy.py",
    "scripts/lushtext-automation.py",
    "scripts/test-visual-geometry.py",
    "scripts/visual-geometry-smoke.py",
    "scripts/visual_geometry_png.py",
];
const VISUAL_SENSITIVE_SUFFIXES: &[&str] = &[".blp", ".css", ".ui"];

// These invariant identifiers are shared with the Python visual runner and the
// generated summaries; changing them would invalidate existing smoke evidence.
const NATIVE_MINIMAP_HIGHLIGHT_INVARIANT: &str = "native-minimap-highlight-anchors";
const NATIVE_MINIMAP_ANIMATION_INVARIANT: &str = "native-minimap-animation-highlight-anchors";

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PolicyOutcome {
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

pub(crate) fn run_self_tests() -> Result<(), String> {
    if !is_visual_sensitive("crates/lushtext-core/src/ui/window/imp.rs")
        || !is_visual_sensitive("crates/lushtext-core/src/model/automation.rs")
        || !is_visual_sensitive("resources/ui/window.blp")
        || !is_visual_sensitive("resources/style/style.css")
        || !is_visual_sensitive("scripts/visual-geometry-scenarios/example.json")
        || !is_visual_sensitive("scripts/check-visual-proof-policy.py")
        || is_visual_sensitive("docs/automation.md")
    {
        return Err("visual-sensitive path classification failed".to_string());
    }
    assert_eq_detail(
        &required_invariants_for_changes(&["resources/style/style.css".to_string()]),
        &[NATIVE_MINIMAP_HIGHLIGHT_INVARIANT.to_string()],
        "minimap pixel invariant mapping failed",
    )?;
    assert_eq_detail(
        &required_animation_invariants_for_changes(&[
            "crates/lushtext-core/src/ui/editor_page/overscroll.rs".to_string(),
        ]),
        &[NATIVE_MINIMAP_ANIMATION_INVARIANT.to_string()],
        "minimap animation invariant mapping failed",
    )?;
    if visual_change_fingerprint(&["docs/automation.md".to_string()])?
        == visual_change_fingerprint(&["missing-visual-proof-file.rs".to_string()])?
    {
        return Err("visual change fingerprint failed to distinguish file sets".to_string());
    }

    let mut summary = serde_json::json!({
        "status": "passed",
        "case_count": 1,
        "passed": 1,
        "failed": 0,
        "skipped": 0,
        "cases": []
    });
    proof_is_verified(&summary)?;
    expect_err_contains(
        proof_matches_current_changes(&summary, &["docs/automation.md".to_string()]),
        "fingerprint",
    )?;

    let digest = visual_change_fingerprint(&["docs/automation.md".to_string()])?;
    summary["visual_proof_policy"] = serde_json::json!({ "changed_files_digest": digest });
    proof_matches_current_changes(&summary, &["docs/automation.md".to_string()])?;
    expect_err_contains(
        proof_covers_required_invariants(&summary, &["resources/style/style.css".to_string()]),
        "pixel_verified_invariant_ids",
    )?;
    summary["verified_invariant_ids"] = serde_json::json!([NATIVE_MINIMAP_HIGHLIGHT_INVARIANT]);
    expect_err_contains(
        proof_covers_required_invariants(&summary, &["resources/style/style.css".to_string()]),
        "pixel_verified_invariant_ids",
    )?;
    summary["pixel_verified_invariant_ids"] =
        serde_json::json!([NATIVE_MINIMAP_HIGHLIGHT_INVARIANT]);
    expect_err_contains(
        proof_covers_required_invariants(&summary, &["resources/style/style.css".to_string()]),
        "pixel-evidence case",
    )?;
    summary["cases"] = serde_json::json!([{
        "status": "passed",
        "pixel_verified_invariant_ids": [NATIVE_MINIMAP_HIGHLIGHT_INVARIANT],
        "pixel_anchor_evidence": [{
            "name": "minimap-native-viewport-top-edge",
            "before_row_y": 10,
            "after_row_y": 10
        }],
        "final_geometry": {
            "before": [{"name": "workspace-sidebar"}],
            "after": [{"name": "workspace-sidebar"}]
        }
    }]);
    proof_covers_required_invariants(&summary, &["resources/style/style.css".to_string()])?;
    expect_err_contains(
        proof_covers_required_invariants(
            &summary,
            &["crates/lushtext-core/src/ui/editor_page/overscroll.rs".to_string()],
        ),
        "animation_verified_invariant_ids",
    )?;
    summary["animation_verified_invariant_ids"] =
        serde_json::json!([NATIVE_MINIMAP_ANIMATION_INVARIANT]);
    expect_err_contains(
        proof_covers_required_invariants(
            &summary,
            &["crates/lushtext-core/src/ui/editor_page/overscroll.rs".to_string()],
        ),
        "animation-evidence case",
    )?;
    summary["cases"][0]["animation_verified_invariant_ids"] =
        serde_json::json!([NATIVE_MINIMAP_ANIMATION_INVARIANT]);
    summary["cases"][0]["animation_frame_evidence"] = serde_json::json!({
        "status": "passed",
        "sampled_frame_count": 2,
        "frames": [{
            "frame_index": 0,
            "status": "passed",
            "anchors": [{
                "name": "minimap-native-viewport-top-edge",
                "status": "passed",
                "baseline_row_y": 10,
                "frame_row_y": 10
            }]
        }]
    });
    expect_err_contains(
        proof_covers_required_invariants(
            &summary,
            &["crates/lushtext-core/src/ui/editor_page/overscroll.rs".to_string()],
        ),
        "animation frame rows",
    )?;
    summary["cases"][0]["animation_frame_evidence"]["capture_mode"] = serde_json::json!("stream");
    summary["cases"][0]["animation_frame_evidence"]["mapped_intermediate_frame_count"] =
        serde_json::json!(1);
    summary["cases"][0]["animation_frame_evidence"]["max_sample_skew_ms"] = serde_json::json!(80);
    summary["cases"][0]["animation_frame_evidence"]["max_sample_skew_observed_ms"] =
        serde_json::json!(12);
    summary["cases"][0]["animation_frame_evidence"]["frames"][0]["mapped_sample_elapsed_ms"] =
        serde_json::json!(48);
    summary["cases"][0]["animation_frame_evidence"]["frames"][0]["sample_skew_ms"] =
        serde_json::json!(12);
    summary["cases"][0]["animation_frame_evidence"]["frames"][0]["sidebar_phase"] =
        serde_json::json!("intermediate");

    let valid_animation = summary["cases"][0]["animation_frame_evidence"].clone();
    for invalid in invalid_animation_evidence_cases(&valid_animation) {
        summary["cases"][0]["animation_frame_evidence"] = invalid;
        expect_err_contains(
            proof_covers_required_invariants(
                &summary,
                &["crates/lushtext-core/src/ui/editor_page/overscroll.rs".to_string()],
            ),
            "animation frame rows",
        )?;
    }
    summary["cases"][0]["animation_frame_evidence"] = valid_animation;
    proof_covers_required_invariants(
        &summary,
        &["crates/lushtext-core/src/ui/editor_page/overscroll.rs".to_string()],
    )?;

    let skipped = serde_json::json!({
        "status": "passed",
        "case_count": 1,
        "passed": 0,
        "failed": 0,
        "skipped": 1
    });
    expect_err_contains(proof_is_verified(&skipped), "skipped")?;
    Ok(())
}

pub(crate) fn check_policy(artifact_dir: &Path, base_ref: Option<&str>) -> PolicyOutcome {
    let changed = changed_files(base_ref);
    let visual_changes: Vec<String> = changed
        .into_iter()
        .filter(|path| is_visual_sensitive(path))
        .collect();
    if visual_changes.is_empty() {
        return PolicyOutcome {
            ok: true,
            detail: "No local visual-sensitive changes require visual geometry proof.".to_string(),
        };
    }

    let summary_path = artifact_dir.join("summary.json");
    let summary = match read_summary(&summary_path) {
        Ok(summary) => summary,
        Err(error) => {
            return PolicyOutcome {
                ok: false,
                detail: visual_failure_detail(
                    "Visual-sensitive changes require same-session visual geometry proof.",
                    &visual_changes,
                    &format!("{error}\nRun `make visual-geometry-smoke` and rerun this check."),
                    &summary_path,
                ),
            };
        }
    };

    let detail = proof_is_verified(&summary)
        .and_then(|detail| {
            proof_matches_current_changes(&summary, &visual_changes)
                .map(|match_detail| format!("{detail}; {match_detail}"))
        })
        .and_then(|detail| {
            proof_covers_required_invariants(&summary, &visual_changes)
                .map(|coverage_detail| format!("{detail}; {coverage_detail}"))
        });

    match detail {
        Ok(detail) => PolicyOutcome {
            ok: true,
            detail: format!("{detail}: {}", summary_path.display()),
        },
        Err(detail) => PolicyOutcome {
            ok: false,
            detail: visual_failure_detail(
                "Visual-sensitive changes require a passing visual geometry proof.",
                &visual_changes,
                &detail,
                &summary_path,
            ),
        },
    }
}

fn assert_eq_detail<T>(left: &[T], right: &[T], detail: &str) -> Result<(), String>
where
    T: Eq + std::fmt::Debug,
{
    if left == right {
        Ok(())
    } else {
        Err(format!("{detail}: left={left:?} right={right:?}"))
    }
}

fn expect_err_contains(result: Result<String, String>, needle: &str) -> Result<(), String> {
    match result {
        Ok(detail) => Err(format!(
            "expected error containing {needle:?}, got ok: {detail}"
        )),
        Err(detail) if detail.contains(needle) => Ok(()),
        Err(detail) => Err(format!(
            "expected error containing {needle:?}, got: {detail}"
        )),
    }
}

fn invalid_animation_evidence_cases(valid: &Value) -> Vec<Value> {
    let mut screenshot = valid.clone();
    screenshot["capture_mode"] = serde_json::json!("screenshot");
    let mut no_intermediate = valid.clone();
    no_intermediate["mapped_intermediate_frame_count"] = serde_json::json!(0);
    let mut stale_skew = valid.clone();
    stale_skew["max_sample_skew_observed_ms"] = serde_json::json!(120);
    let mut unmapped_frame = valid.clone();
    unmapped_frame["frames"][0]["mapped_sample_elapsed_ms"] = Value::Null;
    let mut missing_anchor = valid.clone();
    missing_anchor["frames"][0]["anchors"] = serde_json::json!([]);
    vec![
        screenshot,
        no_intermediate,
        stale_skew,
        unmapped_frame,
        missing_anchor,
    ]
}

fn visual_failure_detail(
    heading: &str,
    visual_changes: &[String],
    detail: &str,
    summary_path: &Path,
) -> String {
    let mut lines = vec![heading.to_string(), "Changed files:".to_string()];
    lines.extend(visual_changes.iter().map(|path| format!("  - {path}")));
    lines.push(detail.to_string());
    lines.push(format!("Artifact summary: {}", summary_path.display()));
    lines.join("\n")
}

fn read_summary(path: &Path) -> Result<Value, String> {
    let value = read_json_value(path, "visual geometry summary")?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(format!(
            "visual geometry summary is not an object: {}",
            path.display()
        ))
    }
}

fn proof_is_verified(summary: &Value) -> Result<String, String> {
    let status = summary.get("status").and_then(Value::as_str);
    let case_count = summary.get("case_count").and_then(Value::as_i64);
    let passed = summary.get("passed").and_then(Value::as_i64);
    let failed = summary.get("failed").and_then(Value::as_i64);
    let skipped = summary.get("skipped").and_then(Value::as_i64);
    if status != Some("passed") {
        return Err(format!("summary status is {status:?}, not 'passed'"));
    }
    if case_count.is_none_or(|count| count <= 0) {
        return Err("summary has no executed visual geometry cases".to_string());
    }
    if failed.is_some_and(|count| count != 0) {
        return Err(format!("summary reports failed cases: {failed:?}"));
    }
    if skipped.is_some_and(|count| count != 0) {
        return Err(format!(
            "summary reports skipped cases: {skipped:?}; skipped coverage is not proof"
        ));
    }
    if passed.is_none_or(|count| count <= 0) {
        return Err("summary reports no passing visual geometry cases".to_string());
    }
    if summary
        .get("case_filter")
        .is_some_and(|filter| !filter.is_null())
    {
        return Err("filtered visual geometry runs do not satisfy visual proof policy".to_string());
    }
    Ok("visual geometry proof summary passed".to_string())
}

fn proof_matches_current_changes(
    summary: &Value,
    visual_changes: &[String],
) -> Result<String, String> {
    let Some(metadata) = summary
        .get("visual_proof_policy")
        .and_then(Value::as_object)
    else {
        return Err(
            "summary has no current-diff fingerprint; rerun visual geometry smoke".to_string(),
        );
    };
    let Some(recorded_digest) = metadata.get("changed_files_digest").and_then(Value::as_str) else {
        return Err("summary has no changed-files digest; rerun visual geometry smoke".to_string());
    };
    let current_digest = visual_change_fingerprint(visual_changes)?;
    if current_digest != recorded_digest {
        return Err(
            "summary changed-files digest does not match current visual-sensitive diff; rerun visual geometry smoke"
                .to_string(),
        );
    }
    Ok("summary matches current visual-sensitive diff".to_string())
}

fn proof_covers_required_invariants(
    summary: &Value,
    visual_changes: &[String],
) -> Result<String, String> {
    let required = required_invariants_for_changes(visual_changes);
    if required.is_empty() {
        return proof_covers_required_animation_invariants(summary, visual_changes);
    }
    let Some(verified) = string_array(summary.get("pixel_verified_invariant_ids")) else {
        return Err(
            "summary has no pixel_verified_invariant_ids; rerun visual geometry smoke".to_string(),
        );
    };
    let missing: Vec<_> = required
        .iter()
        .filter(|required| !verified.contains(*required))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "summary did not pixel-verify required visual invariant ids: {}",
            missing.join(", ")
        ));
    }
    proof_has_required_case_evidence(summary, &required)?;
    let animation_detail = proof_covers_required_animation_invariants(summary, visual_changes)?;
    Ok(format!(
        "summary pixel-verified required visual invariant ids: {}; {animation_detail}",
        required.join(", ")
    ))
}

fn proof_covers_required_animation_invariants(
    summary: &Value,
    visual_changes: &[String],
) -> Result<String, String> {
    let required = required_animation_invariants_for_changes(visual_changes);
    if required.is_empty() {
        return Ok("no animation-frame invariants required by current diff".to_string());
    }
    let Some(verified) = string_array(summary.get("animation_verified_invariant_ids")) else {
        return Err(
            "summary has no animation_verified_invariant_ids; rerun visual geometry smoke with animation sampling"
                .to_string(),
        );
    };
    let missing: Vec<_> = required
        .iter()
        .filter(|required| !verified.contains(*required))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "summary did not animation-verify required visual invariant ids: {}",
            missing.join(", ")
        ));
    }
    proof_has_required_animation_case_evidence(summary, &required)?;
    Ok(format!(
        "summary animation-verified required visual invariant ids: {}",
        required.join(", ")
    ))
}

fn proof_has_required_case_evidence(
    summary: &Value,
    required: &[String],
) -> Result<String, String> {
    let Some(cases) = summary.get("cases").and_then(Value::as_array) else {
        return Err(
            "summary has no case rows with pixel evidence; rerun visual geometry smoke".to_string(),
        );
    };
    for invariant_id in required {
        let matching_cases: Vec<_> = cases
            .iter()
            .filter(|case| {
                case.get("status").and_then(Value::as_str) == Some("passed")
                    && string_array(case.get("pixel_verified_invariant_ids"))
                        .is_some_and(|ids| ids.contains(invariant_id))
            })
            .collect();
        if matching_cases.is_empty() {
            return Err(format!(
                "summary has no passing pixel-evidence case for {invariant_id}"
            ));
        }
        if !matching_cases
            .iter()
            .any(|case| case_has_actionable_pixel_evidence(case))
        {
            return Err(format!(
                "summary case for {invariant_id} lacks pixel rows or final geometry"
            ));
        }
    }
    Ok("required visual invariant cases include pixel rows and final geometry".to_string())
}

fn proof_has_required_animation_case_evidence(
    summary: &Value,
    required: &[String],
) -> Result<String, String> {
    let Some(cases) = summary.get("cases").and_then(Value::as_array) else {
        return Err(
            "summary has no case rows with animation evidence; rerun visual geometry smoke"
                .to_string(),
        );
    };
    for invariant_id in required {
        let matching_cases: Vec<_> = cases
            .iter()
            .filter(|case| {
                case.get("status").and_then(Value::as_str) == Some("passed")
                    && string_array(case.get("animation_verified_invariant_ids"))
                        .is_some_and(|ids| ids.contains(invariant_id))
            })
            .collect();
        if matching_cases.is_empty() {
            return Err(format!(
                "summary has no passing animation-evidence case for {invariant_id}"
            ));
        }
        if !matching_cases
            .iter()
            .any(|case| case_has_actionable_animation_evidence(case))
        {
            return Err(format!(
                "summary case for {invariant_id} lacks animation frame rows"
            ));
        }
    }
    Ok("required visual animation cases include sampled frame rows".to_string())
}

fn case_has_actionable_pixel_evidence(case: &Value) -> bool {
    let Some(evidence) = case.get("pixel_anchor_evidence").and_then(Value::as_array) else {
        return false;
    };
    if evidence.is_empty() || !case.get("final_geometry").is_some_and(Value::is_object) {
        return false;
    }
    evidence.iter().any(|row| {
        row.is_object()
            && row
                .get("before_row_y")
                .is_some_and(|value| !value.is_null())
            && row.get("after_row_y").is_some_and(|value| !value.is_null())
    })
}

fn case_has_actionable_animation_evidence(case: &Value) -> bool {
    let Some(evidence) = case
        .get("animation_frame_evidence")
        .and_then(Value::as_object)
    else {
        return false;
    };
    if evidence.get("status").and_then(Value::as_str) != Some("passed")
        || evidence.get("capture_mode").and_then(Value::as_str) != Some("stream")
        || !positive_i64(evidence.get("sampled_frame_count"))
        || !positive_i64(evidence.get("mapped_intermediate_frame_count"))
    {
        return false;
    }
    let Some(max_skew) = evidence.get("max_sample_skew_ms").and_then(Value::as_i64) else {
        return false;
    };
    let Some(observed_skew) = evidence
        .get("max_sample_skew_observed_ms")
        .and_then(Value::as_i64)
    else {
        return false;
    };
    if observed_skew > max_skew {
        return false;
    }
    let Some(frames) = evidence.get("frames").and_then(Value::as_array) else {
        return false;
    };
    let mut has_mapped_intermediate_anchor = false;
    for frame in frames {
        if frame.get("status").and_then(Value::as_str) != Some("passed")
            || frame
                .get("mapped_sample_elapsed_ms")
                .is_none_or(Value::is_null)
        {
            return false;
        }
        let Some(sample_skew) = frame.get("sample_skew_ms").and_then(Value::as_i64) else {
            return false;
        };
        if sample_skew > max_skew {
            return false;
        }
        let has_passed_anchor = frame
            .get("anchors")
            .and_then(Value::as_array)
            .is_some_and(|anchors| anchors.iter().any(anchor_passed_with_rows));
        if frame.get("sidebar_phase").and_then(Value::as_str) == Some("intermediate")
            && has_passed_anchor
        {
            has_mapped_intermediate_anchor = true;
        }
    }
    has_mapped_intermediate_anchor
}

fn anchor_passed_with_rows(anchor: &Value) -> bool {
    anchor.get("status").and_then(Value::as_str) == Some("passed")
        && anchor
            .get("baseline_row_y")
            .is_some_and(|value| !value.is_null())
        && anchor
            .get("frame_row_y")
            .is_some_and(|value| !value.is_null())
}

fn positive_i64(value: Option<&Value>) -> bool {
    value.and_then(Value::as_i64).is_some_and(|value| value > 0)
}

fn string_array(value: Option<&Value>) -> Option<Vec<String>> {
    value.and_then(Value::as_array).map(|items| {
        items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect()
    })
}

fn is_visual_sensitive(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    VISUAL_SENSITIVE_EXACT.contains(&normalized.as_str())
        || VISUAL_SENSITIVE_PREFIXES
            .iter()
            .any(|prefix| normalized.starts_with(prefix))
        || VISUAL_SENSITIVE_SUFFIXES
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

fn required_invariants_for_changes(paths: &[String]) -> Vec<String> {
    let mut required = Vec::new();
    for path in paths.iter().map(|item| item.replace('\\', "/")) {
        if (path == "crates/lushtext-core/src/ui/editor_page/minimap.rs"
            || path == "crates/lushtext-core/src/ui/automation.rs"
            || path == "crates/lushtext-core/src/model/automation.rs"
            || path == "resources/style/style.css"
            || path == "scripts/check-visual-proof-policy.py"
            || path == "scripts/lushtext-automation.py"
            || path == "scripts/test-visual-geometry.py"
            || path == "scripts/visual-geometry-smoke.py"
            || path == "scripts/visual_geometry_png.py"
            || path.starts_with("scripts/visual-geometry-scenarios/minimap-sidebar-"))
            && !required
                .iter()
                .any(|item| item == NATIVE_MINIMAP_HIGHLIGHT_INVARIANT)
        {
            required.push(NATIVE_MINIMAP_HIGHLIGHT_INVARIANT.to_string());
        }
    }
    required.sort();
    required
}

fn required_animation_invariants_for_changes(paths: &[String]) -> Vec<String> {
    let mut required = Vec::new();
    for path in paths.iter().map(|item| item.replace('\\', "/")) {
        if (path == "crates/lushtext-core/src/ui/editor_page/imp.rs"
            || path == "crates/lushtext-core/src/ui/editor_page/minimap.rs"
            || path == "crates/lushtext-core/src/ui/editor_page/overscroll.rs"
            || path == "crates/lushtext-core/src/ui/automation.rs"
            || path == "crates/lushtext-core/src/model/automation.rs"
            || path == "scripts/check-visual-proof-policy.py"
            || path == "scripts/lushtext-automation.py"
            || path == "scripts/test-visual-geometry.py"
            || path == "scripts/visual-geometry-smoke.py"
            || path == "scripts/visual_geometry_png.py"
            || path.starts_with("scripts/visual-geometry-scenarios/minimap-sidebar-"))
            && !required
                .iter()
                .any(|item| item == NATIVE_MINIMAP_ANIMATION_INVARIANT)
        {
            required.push(NATIVE_MINIMAP_ANIMATION_INVARIANT.to_string());
        }
    }
    required.sort();
    required
}

fn visual_change_fingerprint(paths: &[String]) -> Result<String, String> {
    let root = repo_root();
    let mut entries = Vec::new();
    let mut sorted = paths
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<Vec<_>>();
    sorted.sort();
    sorted.dedup();
    for path in sorted {
        let absolute = root.join(&path);
        let mut entry = serde_json::json!({ "path": path });
        if absolute.is_file() {
            match hash_file_sha256(&absolute) {
                Ok((size, digest)) => {
                    entry["state"] = serde_json::json!("file");
                    entry["size"] = serde_json::json!(size);
                    entry["sha256"] = serde_json::json!(digest);
                }
                Err(error) => {
                    entry["state"] = serde_json::json!("error");
                    entry["error"] = serde_json::json!(error.kind().to_string());
                }
            }
        } else if absolute.exists() {
            entry["state"] = serde_json::json!("non-file");
        } else {
            entry["state"] = serde_json::json!("missing");
        }
        entries.push(entry);
    }
    let encoded = serde_json::to_vec(&entries).map_err(|error| error.to_string())?;
    Ok(hex_sha256(&encoded))
}

fn hash_file_sha256(path: &Path) -> Result<(u64, String), std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += u64::try_from(count).unwrap_or(0);
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    Ok((
        size,
        digest.iter().map(|byte| format!("{byte:02x}")).collect(),
    ))
}

fn hex_sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn changed_files(base_ref: Option<&str>) -> Vec<String> {
    if let Some(base_ref) = base_ref {
        let files = base_ref_changed_files(base_ref);
        if !files.is_empty() {
            return files;
        }
    }
    std::env::var("GITHUB_BASE_REF")
        .ok()
        .and_then(|base_ref| {
            [format!("origin/{base_ref}"), base_ref]
                .into_iter()
                .map(|candidate| base_ref_changed_files(&candidate))
                .find(|files| !files.is_empty())
        })
        .unwrap_or_else(status_changed_files)
}

fn base_ref_changed_files(base_ref: &str) -> Vec<String> {
    let files = run_git(&["diff", "--name-only", &format!("{base_ref}...HEAD")]);
    if files.is_empty() {
        run_git(&["diff", "--name-only", base_ref, "HEAD"])
    } else {
        files
    }
}

fn status_changed_files() -> Vec<String> {
    let mut files = run_git(&["status", "--porcelain=v1", "--untracked-files=all"])
        .into_iter()
        .filter_map(|line| {
            let path = line.get(3..)?;
            Some(
                path.split_once(" -> ")
                    .map_or(path, |(_old, new)| new)
                    .to_string(),
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

fn run_git(args: &[&str]) -> Vec<String> {
    let Ok(output) = Command::new("git")
        .args(args)
        .current_dir(repo_root())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_tests_cover_policy_negative_cases() {
        run_self_tests().expect("policy self-tests");
    }
}
