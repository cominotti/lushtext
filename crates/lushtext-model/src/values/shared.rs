// SPDX-License-Identifier: GPL-3.0-or-later

//! Cold-path reference-counted metadata wrappers (`SharedPath`, `SharedStr`, `SharedPaths`).
//!
//! # Two-Zone Boundary
//!
//! These types live outside Kani proof harnesses (`kani_proofs.rs`) and outside
//! pure `ui/**/policy.rs` geometry/state rules (`scripts/check-arc-safety.py`).
//! Use them in cold orchestration, UI snapshots, worker payloads, and directory
//! metadata where cloning immutable values across GTK closures and threads
//! would otherwise duplicate `PathBuf` or `String` heap buffers.
//!
//! Document-sized text or byte payloads (`> L_PROTECT_EDIT`) must **not** use
//! `SharedStr`; wrap those in `DisposalOwned<T>` (`ui::plain_disposal`) so
//! terminal deallocation runs off the GTK main thread.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Cheap-to-clone immutable filesystem path (`Arc<Path>` without `PathBuf` double indirection).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedPath(Arc<Path>);

impl SharedPath {
    /// Wrap a path slice into a single reference-counted allocation.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(Arc::from(path.as_ref()))
    }

    /// Borrow as `&Path`.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Copy into a mutable owned `PathBuf` only when an external API requires ownership.
    #[must_use]
    pub fn to_path_buf(&self) -> PathBuf {
        self.0.to_path_buf()
    }

    /// Return `true` when both wrappers share the exact same `Arc` allocation.
    #[must_use]
    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.0, &right.0)
    }

    /// Conservative retained heap bytes for the single `Arc<Path>` allocation
    /// (strong + weak counters plus path byte length).
    #[must_use]
    pub fn heap_bytes(&self) -> u64 {
        let bytes = std::mem::size_of::<usize>()
            .saturating_mul(2)
            .saturating_add(self.0.as_os_str().len());
        u64::try_from(bytes).unwrap_or(u64::MAX)
    }
}

impl Default for SharedPath {
    fn default() -> Self {
        Self(Arc::from(Path::new("")))
    }
}

impl Deref for SharedPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for SharedPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for SharedPath {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Borrow<Path> for SharedPath {
    fn borrow(&self) -> &Path {
        &self.0
    }
}

impl From<&Path> for SharedPath {
    fn from(path: &Path) -> Self {
        Self(Arc::from(path))
    }
}

impl From<PathBuf> for SharedPath {
    fn from(path: PathBuf) -> Self {
        Self(Arc::from(path))
    }
}

impl From<&PathBuf> for SharedPath {
    fn from(path: &PathBuf) -> Self {
        Self(Arc::from(path.as_path()))
    }
}

impl From<Arc<Path>> for SharedPath {
    fn from(path: Arc<Path>) -> Self {
        Self(path)
    }
}

impl From<&str> for SharedPath {
    fn from(path: &str) -> Self {
        Self(Arc::from(Path::new(path)))
    }
}

impl From<String> for SharedPath {
    fn from(path: String) -> Self {
        Self(Arc::from(PathBuf::from(path)))
    }
}

impl From<SharedPath> for PathBuf {
    fn from(path: SharedPath) -> Self {
        path.0.to_path_buf()
    }
}

impl Serialize for SharedPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SharedPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = PathBuf::deserialize(deserializer)?;
        Ok(Self::from(path))
    }
}

/// Cheap-to-clone immutable bounded metadata string (`Arc<str>` without `String` double indirection).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedStr(Arc<str>);

impl SharedStr {
    /// Wrap a string slice into a single reference-counted allocation.
    #[must_use]
    pub fn new(text: impl AsRef<str>) -> Self {
        Self(Arc::from(text.as_ref()))
    }

    /// Borrow as `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return `true` when both wrappers share the exact same `Arc` allocation.
    #[must_use]
    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.0, &right.0)
    }
}

impl Default for SharedStr {
    fn default() -> Self {
        Self(Arc::from(""))
    }
}

impl Deref for SharedStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for SharedStr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for SharedStr {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SharedStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SharedStr {
    fn from(text: &str) -> Self {
        Self(Arc::from(text))
    }
}

impl From<String> for SharedStr {
    fn from(text: String) -> Self {
        Self(Arc::from(text))
    }
}

impl From<&String> for SharedStr {
    fn from(text: &String) -> Self {
        Self(Arc::from(text.as_str()))
    }
}

impl From<Arc<str>> for SharedStr {
    fn from(text: Arc<str>) -> Self {
        Self(text)
    }
}

impl From<SharedStr> for String {
    fn from(text: SharedStr) -> Self {
        text.0.to_string()
    }
}

impl Serialize for SharedStr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SharedStr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self::from(text))
    }
}

/// Immutable reference-counted slice of [`SharedPath`] workspace folders or path sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedPaths(Arc<[SharedPath]>);

impl SharedPaths {
    /// Wrap a slice/iterator of paths into one shared slice allocation.
    #[must_use]
    pub fn from_paths<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<SharedPath>,
    {
        let items: Vec<SharedPath> = paths.into_iter().map(Into::into).collect();
        Self(Arc::from(items))
    }

    /// Borrow the underlying path slice.
    #[must_use]
    pub fn as_slice(&self) -> &[SharedPath] {
        &self.0
    }

    /// Iterate over shared path entries.
    pub fn iter(&self) -> std::slice::Iter<'_, SharedPath> {
        self.0.iter()
    }

    /// Return `true` when both wrappers share the exact same underlying slice allocation.
    #[must_use]
    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.0, &right.0)
    }
}

impl Deref for SharedPaths {
    type Target = [SharedPath];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[SharedPath]> for SharedPaths {
    fn as_ref(&self) -> &[SharedPath] {
        &self.0
    }
}

impl From<Vec<PathBuf>> for SharedPaths {
    fn from(paths: Vec<PathBuf>) -> Self {
        Self::from_paths(paths)
    }
}

impl From<Vec<SharedPath>> for SharedPaths {
    fn from(paths: Vec<SharedPath>) -> Self {
        Self(Arc::from(paths))
    }
}

impl From<&[PathBuf]> for SharedPaths {
    fn from(paths: &[PathBuf]) -> Self {
        Self::from_paths(paths.iter())
    }
}

impl<const N: usize> From<[PathBuf; N]> for SharedPaths {
    fn from(paths: [PathBuf; N]) -> Self {
        Self::from_paths(paths)
    }
}

impl<const N: usize> From<[SharedPath; N]> for SharedPaths {
    fn from(paths: [SharedPath; N]) -> Self {
        Self(Arc::from(paths))
    }
}

impl<'a> IntoIterator for &'a SharedPaths {
    type Item = &'a SharedPath;
    type IntoIter = std::slice::Iter<'a, SharedPath>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn shared_path_borrow_and_ptr_eq_work_without_allocation() {
        let a = SharedPath::from("/workspace/main.rs");
        let b = a.clone();
        assert!(SharedPath::ptr_eq(&a, &b));
        assert_eq!(a.as_path(), Path::new("/workspace/main.rs"));

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(Path::new("/workspace/main.rs")));
    }

    #[test]
    fn shared_str_borrow_display_and_serde_roundtrip() {
        let s = SharedStr::from("workspace-label");
        let s2 = s.clone();
        assert!(SharedStr::ptr_eq(&s, &s2));
        assert_eq!(s.to_string(), "workspace-label");

        let mut map = HashMap::new();
        map.insert(s.clone(), 42);
        assert_eq!(map.get("workspace-label"), Some(&42));

        let json = serde_json::to_string(&s).expect("serialize SharedStr");
        assert_eq!(json, "\"workspace-label\"");
        let parsed: SharedStr = serde_json::from_str(&json).expect("deserialize SharedStr");
        assert_eq!(parsed, s);
    }

    #[test]
    fn shared_path_serde_and_shared_paths_ptr_eq() {
        let path = SharedPath::from(PathBuf::from("/tmp/example.txt"));
        let json = serde_json::to_string(&path).expect("serialize SharedPath");
        let roundtrip: SharedPath = serde_json::from_str(&json).expect("deserialize SharedPath");
        assert_eq!(roundtrip, path);

        let paths = SharedPaths::from([PathBuf::from("/w/a"), PathBuf::from("/w/b")]);
        let same = paths.clone();
        let different = SharedPaths::from([PathBuf::from("/w/a")]);
        assert!(SharedPaths::ptr_eq(&paths, &same));
        assert!(!SharedPaths::ptr_eq(&paths, &different));
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].as_path(), Path::new("/w/a"));
    }
}
