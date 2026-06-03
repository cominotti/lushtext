// SPDX-License-Identifier: GPL-3.0-or-later

//! Private backend for raw filesystem APIs.
//!
//! This is the intentional exception to the repository-wide direct filesystem
//! ban. Keeping `std::fs`, Unix extension traits, `libc`, and `rustix` here
//! lets the public boundary stay readable while this module owns low-level
//! descriptor handling and platform details.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub(super) type Metadata = fs::Metadata;

pub(super) struct RawDirectoryEntry {
    pub path: PathBuf,
    pub file_name: OsString,
    pub metadata: fs::Metadata,
}

pub(super) fn read(path: &Path) -> io::Result<Vec<u8>> {
    fs::read(path)
}

pub(super) fn read_to_string(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

pub(super) fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    fs::write(path, contents)
}

pub(super) fn create_new_empty_file(path: &Path) -> io::Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.sync_all()
}

pub(super) fn metadata(path: &Path) -> io::Result<fs::Metadata> {
    fs::metadata(path)
}

pub(super) fn symlink_metadata(path: &Path) -> io::Result<fs::Metadata> {
    fs::symlink_metadata(path)
}

pub(super) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    path.canonicalize()
}

#[cfg(unix)]
pub(super) fn create_dir(path: &Path) -> io::Result<()> {
    rustix::fs::mkdirat(rustix::fs::CWD, path, rustix::fs::Mode::from(0o777))
        .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(super) fn create_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

pub(super) fn create_dir_all(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
pub(super) fn remove_file(path: &Path) -> io::Result<()> {
    rustix::fs::unlinkat(rustix::fs::CWD, path, rustix::fs::AtFlags::empty())
        .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(super) fn remove_file(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

#[cfg(unix)]
pub(super) fn remove_dir(path: &Path) -> io::Result<()> {
    rustix::fs::unlinkat(rustix::fs::CWD, path, rustix::fs::AtFlags::REMOVEDIR)
        .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(super) fn remove_dir(path: &Path) -> io::Result<()> {
    fs::remove_dir(path)
}

pub(super) fn remove_dir_all(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[cfg(unix)]
pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    rustix::fs::renameat(rustix::fs::CWD, from, rustix::fs::CWD, to).map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

pub(super) fn create_sparse_file(path: &Path, len: u64) -> io::Result<()> {
    let file = File::create(path)?;
    file.set_len(len)
}

pub(super) fn write_at_start(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(bytes)
}

pub(super) fn read_prefix(path: &Path, byte_limit: usize) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut limited = Read::by_ref(&mut file).take(u64::try_from(byte_limit).unwrap_or(u64::MAX));
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(unix)]
pub(super) fn visit_directory_entries<F>(path: &Path, mut visit: F) -> io::Result<()>
where
    F: FnMut(RawDirectoryEntry) -> bool,
{
    use std::os::unix::ffi::OsStringExt;

    let fd = rustix::fs::openat(
        rustix::fs::CWD,
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut dir = rustix::fs::Dir::new(fd).map_err(io::Error::from)?;

    while let Some(entry) = dir.read() {
        let entry = entry.map_err(io::Error::from)?;
        let name_bytes = entry.file_name().to_bytes();
        if name_bytes == b"." || name_bytes == b".." {
            continue;
        }
        let file_name = OsString::from_vec(name_bytes.to_vec());
        let child_path = path.join(&file_name);
        let Ok(metadata) = metadata(&child_path) else {
            continue;
        };
        if !visit(RawDirectoryEntry {
            path: child_path,
            file_name,
            metadata,
        }) {
            break;
        }
    }

    Ok(())
}

#[cfg(not(unix))]
pub(super) fn visit_directory_entries<F>(path: &Path, mut visit: F) -> io::Result<()>
where
    F: FnMut(RawDirectoryEntry) -> bool,
{
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !visit(RawDirectoryEntry {
            path: entry.path(),
            file_name: entry.file_name(),
            metadata,
        }) {
            break;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
pub(super) fn symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlink fixtures require Unix",
    ))
}

#[cfg(unix)]
pub(super) fn set_permissions_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub(super) fn set_permissions_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "permission-mode fixtures require Unix",
    ))
}

#[cfg(unix)]
pub(super) fn mode(path: &Path) -> io::Result<u32> {
    use std::os::unix::fs::PermissionsExt;

    Ok(metadata(path)?.permissions().mode())
}

#[cfg(unix)]
pub(super) fn inode(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;

    Ok(metadata(path)?.ino())
}

#[cfg(not(unix))]
pub(super) fn mode(_path: &Path) -> io::Result<u32> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "permission-mode queries require Unix",
    ))
}

#[cfg(not(unix))]
pub(super) fn inode(_path: &Path) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "inode queries require Unix",
    ))
}

#[cfg(unix)]
pub(super) fn descriptor_file_len(path: &Path) -> io::Result<u64> {
    let stat = rustix::fs::stat(path).map_err(io::Error::from)?;
    u64::try_from(stat.st_size).map_err(|_| io::Error::other("negative file size"))
}

#[cfg(not(unix))]
pub(super) fn descriptor_file_len(path: &Path) -> io::Result<u64> {
    Ok(metadata(path)?.len())
}

#[cfg(unix)]
pub(super) fn sync_dir_descriptor(path: &Path) -> io::Result<()> {
    let fd = rustix::fs::openat(
        rustix::fs::CWD,
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(io::Error::from)?;
    rustix::fs::fsync(&fd).map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(super) fn sync_dir_descriptor(_path: &Path) -> io::Result<()> {
    Ok(())
}
