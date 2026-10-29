// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic read-permission failure for the real-process smoke lanes.
//!
//! The accessibility and visual smokes need a document whose load fails with
//! "Permission denied", so the "Could Not Open File" alert appears. A mode-000
//! fixture only fails for an unprivileged user: the scheduled smoke workflow
//! runs its Fedora containers as root, which reads the file anyway (root's
//! `CAP_DAC_OVERRIDE`), so those cases waited for an alert that never came.
//! Failure-path fixtures must not depend on ambient Unix permissions (see the
//! "Property Testing" section of `.agents/rules/build.md`).
//!
//! With the `smoke-read-faults` Cargo feature, [`denied`] reads
//! `LUSHTEXT_DENY_READ` once and reports `EACCES` for a document load whose
//! path (as opened, or canonical) equals it exactly, the same error the
//! kernel returns for an unreadable file, so the editor shows the identical
//! alert and status text. Without the feature (every release, Meson, Flatpak,
//! and Snap build, and the ordinary debug binary) `denied` is an empty inline
//! function and no environment read is compiled.

use std::io;
use std::path::Path;

/// The injected failure for a document load of `path` or `canonical_path`,
/// if the smoke named that file in `LUSHTEXT_DENY_READ`.
#[cfg(feature = "smoke-read-faults")]
#[must_use]
pub fn denied(path: &Path, canonical_path: Option<&Path>) -> Option<io::Error> {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static TARGET: OnceLock<Option<PathBuf>> = OnceLock::new();
    let target = TARGET.get_or_init(|| std::env::var_os("LUSHTEXT_DENY_READ").map(PathBuf::from));
    let target = target.as_deref()?;
    if target != path && Some(target) != canonical_path {
        return None;
    }
    eprintln!(
        "LUSHTEXT_DENY_READ: denying the document read of {}",
        path.display()
    );
    Some(io::Error::from_raw_os_error(EACCES))
}

/// `EACCES`, the errno an unreadable file's `open` fails with on Linux. Raw OS
/// errors keep the kernel's own message ("Permission denied (os error 13)"),
/// which the smoke lanes' warning allowlists match exactly.
#[cfg(feature = "smoke-read-faults")]
const EACCES: i32 = 13;

/// The injected failure for a document load; compiled out without the
/// `smoke-read-faults` feature.
#[cfg(not(feature = "smoke-read-faults"))]
#[inline(always)]
#[must_use]
pub fn denied(_path: &Path, _canonical_path: Option<&Path>) -> Option<io::Error> {
    None
}

#[cfg(all(test, not(feature = "smoke-read-faults")))]
mod tests {
    use std::path::Path;

    #[test]
    fn builds_without_the_feature_never_deny_a_read() {
        let path = Path::new("/any/document.md");
        assert!(super::denied(path, Some(path)).is_none());
    }
}
