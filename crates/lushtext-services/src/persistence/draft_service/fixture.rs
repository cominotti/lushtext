// SPDX-License-Identifier: GPL-3.0-or-later

//! Test and benchmark seeding for draft bodies.
//!
//! Production body writes take a [`super::RegisteredDraft`], so a body is never
//! written for an id the journal cannot explain. Fixtures deliberately need the
//! opposite — orphan bodies, crash leftovers, bodies seeded before their
//! manifest — so they write through here. The module is compiled only under
//! `cfg(test)` or the `test-utils` feature, so production code cannot call it.

use std::path::Path;

use anyhow::Result;

/// Write one draft body with no registration check, for test and benchmark
/// fixtures only.
///
/// # Errors
///
/// Returns an error if the drafts directory cannot be created or the body
/// cannot be durably written.
pub fn write_body(data_dir: &Path, draft_id: &str, content: &str) -> Result<()> {
    super::write_body_file(data_dir, draft_id, content)
}
