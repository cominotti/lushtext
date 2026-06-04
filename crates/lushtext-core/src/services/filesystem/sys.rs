// SPDX-License-Identifier: GPL-3.0-or-later

//! Private backend for raw filesystem APIs.
//!
//! This is the intentional exception to the repository-wide direct filesystem
//! ban. Keeping `std::fs`, Unix extension traits, and `rustix` here lets the
//! public boundary stay readable while this module owns low-level descriptor
//! handling and platform details.

use std::ffi::{CString, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub(in crate::services) type File = fs::File;
pub(in crate::services) type Metadata = fs::Metadata;

#[cfg(unix)]
#[derive(Clone, Debug)]
pub(in crate::services) struct UnixMetadata {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

pub(in crate::services) struct RawDirectoryEntry {
    pub path: PathBuf,
    pub file_name: OsString,
    pub metadata: fs::Metadata,
}

pub(in crate::services) fn read(path: &Path) -> io::Result<Vec<u8>> {
    fs::read(path)
}

pub(in crate::services) fn read_to_string(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

pub(in crate::services) fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    fs::write(path, contents)
}

pub(in crate::services) fn create_new_empty_file(path: &Path) -> io::Result<()> {
    let file = create_temp_file(path, None)?;
    sync_file(&file)
}

pub(in crate::services) fn metadata(path: &Path) -> io::Result<fs::Metadata> {
    fs::metadata(path)
}

pub(in crate::services) fn symlink_metadata(path: &Path) -> io::Result<fs::Metadata> {
    fs::symlink_metadata(path)
}

pub(in crate::services) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

pub(in crate::services) fn path_exists(path: &Path) -> bool {
    metadata(path).is_ok()
}

#[cfg(unix)]
pub(in crate::services) fn create_dir(path: &Path) -> io::Result<()> {
    rustix::fs::mkdirat(
        rustix::fs::CWD,
        path,
        rustix::fs::Mode::from_raw_mode(0o777),
    )
    .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn create_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

pub(in crate::services) fn create_dir_all(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
pub(in crate::services) fn remove_file(path: &Path) -> io::Result<()> {
    rustix::fs::unlinkat(rustix::fs::CWD, path, rustix::fs::AtFlags::empty())
        .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn remove_file(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

#[cfg(unix)]
pub(in crate::services) fn remove_dir(path: &Path) -> io::Result<()> {
    rustix::fs::unlinkat(rustix::fs::CWD, path, rustix::fs::AtFlags::REMOVEDIR)
        .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn remove_dir(path: &Path) -> io::Result<()> {
    fs::remove_dir(path)
}

pub(in crate::services) fn remove_dir_all(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[cfg(unix)]
pub(in crate::services) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    rustix::fs::renameat(rustix::fs::CWD, from, rustix::fs::CWD, to).map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

pub(in crate::services) fn create_sparse_file(path: &Path, len: u64) -> io::Result<()> {
    let file = fs::File::create(path)?;
    file.set_len(len)
}

pub(in crate::services) fn write_at_start(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(bytes)
}

pub(in crate::services) fn read_prefix(path: &Path, byte_limit: usize) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut limited = Read::by_ref(&mut file).take(u64::try_from(byte_limit).unwrap_or(u64::MAX));
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(unix)]
pub(in crate::services) fn visit_directory_entries<F>(path: &Path, mut visit: F) -> io::Result<()>
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
pub(in crate::services) fn visit_directory_entries<F>(path: &Path, mut visit: F) -> io::Result<()>
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
pub(in crate::services) fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    rustix::fs::symlinkat(target, rustix::fs::CWD, link).map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlink fixtures require Unix",
    ))
}

#[cfg(unix)]
pub(in crate::services) fn set_permissions_mode(path: &Path, mode: u32) -> io::Result<()> {
    rustix::fs::chmodat(
        rustix::fs::CWD,
        path,
        rustix::fs::Mode::from_raw_mode(mode),
        rustix::fs::AtFlags::empty(),
    )
    .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn set_permissions_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "permission-mode fixtures require Unix",
    ))
}

#[cfg(unix)]
pub(in crate::services) fn mode(path: &Path) -> io::Result<u32> {
    rustix::fs::stat(path)
        .map(|stat| stat.st_mode)
        .map_err(io::Error::from)
}

#[cfg(unix)]
pub(in crate::services) fn inode(path: &Path) -> io::Result<u64> {
    rustix::fs::stat(path)
        .map(|stat| stat.st_ino)
        .map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn mode(_path: &Path) -> io::Result<u32> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "permission-mode queries require Unix",
    ))
}

#[cfg(not(unix))]
pub(in crate::services) fn inode(_path: &Path) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "inode queries require Unix",
    ))
}

#[cfg(unix)]
pub(in crate::services) fn descriptor_file_len(path: &Path) -> io::Result<u64> {
    let stat = rustix::fs::stat(path).map_err(io::Error::from)?;
    u64::try_from(stat.st_size).map_err(|_| io::Error::other("negative file size"))
}

#[cfg(not(unix))]
pub(in crate::services) fn descriptor_file_len(path: &Path) -> io::Result<u64> {
    Ok(metadata(path)?.len())
}

#[cfg(unix)]
pub(in crate::services) fn sync_dir_descriptor(path: &Path) -> io::Result<()> {
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
pub(in crate::services) fn sync_dir_descriptor(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(in crate::services) fn create_temp_file(path: &Path, mode: Option<u32>) -> io::Result<File> {
    let fd = rustix::fs::openat(
        rustix::fs::CWD,
        path,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::from_raw_mode(mode.unwrap_or(0o666)),
    )
    .map_err(io::Error::from)?;
    Ok(File::from(fd))
}

#[cfg(not(unix))]
pub(in crate::services) fn create_temp_file(path: &Path, _mode: Option<u32>) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(unix)]
pub(in crate::services) fn sync_file(file: &File) -> io::Result<()> {
    rustix::fs::fsync(file).map_err(io::Error::from)
}

#[cfg(not(unix))]
pub(in crate::services) fn sync_file(file: &File) -> io::Result<()> {
    file.sync_all()
}

#[cfg(unix)]
pub(in crate::services) fn required_metadata(path: &Path) -> io::Result<UnixMetadata> {
    let stat = rustix::fs::stat(path).map_err(io::Error::from)?;
    Ok(UnixMetadata {
        mode: stat.st_mode,
        uid: stat.st_uid,
        gid: stat.st_gid,
    })
}

#[cfg(unix)]
pub(in crate::services) fn apply_mode(file: &File, mode: u32) -> io::Result<()> {
    rustix::fs::fchmod(file, rustix::fs::Mode::from_raw_mode(mode)).map_err(io::Error::from)
}

#[cfg(unix)]
pub(in crate::services) fn best_effort_chown(file: &File, uid: u32, gid: u32) {
    let _ = rustix::fs::fchown(
        file,
        Some(rustix::fs::Uid::from_raw(uid)),
        Some(rustix::fs::Gid::from_raw(gid)),
    );
}

#[cfg(target_os = "linux")]
pub(in crate::services) fn copy_xattrs_best_effort(source: &Path, dest: &File) {
    let Ok(names) = xattr_names(source) else {
        return;
    };

    for name in names
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
    {
        let Ok(name) = CString::new(name) else {
            continue;
        };
        let Ok(value) = get_xattr_with_name(source, name.as_c_str()) else {
            continue;
        };
        let _ = rustix::fs::fsetxattr(
            dest,
            name.as_c_str(),
            &value,
            rustix::fs::XattrFlags::empty(),
        );
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
pub(in crate::services) fn copy_xattrs_best_effort(_source: &Path, _dest: &File) {}

#[cfg(target_os = "linux")]
pub(in crate::services) fn set_xattr(path: &Path, name: &str, value: &[u8]) -> io::Result<()> {
    rustix::fs::setxattr(path, name, value, rustix::fs::XattrFlags::empty())
        .map_err(io::Error::from)
}

#[cfg(not(target_os = "linux"))]
pub(in crate::services) fn set_xattr(_path: &Path, _name: &str, _value: &[u8]) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "xattr fixtures require Linux",
    ))
}

#[cfg(target_os = "linux")]
pub(in crate::services) fn get_xattr(path: &Path, name: &str) -> io::Result<Vec<u8>> {
    get_xattr_with_name(path, name)
}

#[cfg(not(target_os = "linux"))]
pub(in crate::services) fn get_xattr(_path: &Path, _name: &str) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "xattr fixtures require Linux",
    ))
}

#[cfg(target_os = "linux")]
fn xattr_names(path: &Path) -> io::Result<Vec<u8>> {
    let mut empty = [0u8; 0];
    let len = rustix::fs::listxattr(path, &mut empty).map_err(io::Error::from)?;
    if len == 0 {
        return Ok(Vec::new());
    }

    let mut names = vec![0u8; len];
    let written = rustix::fs::listxattr(path, &mut names).map_err(io::Error::from)?;
    names.truncate(written);
    Ok(names)
}

#[cfg(target_os = "linux")]
fn get_xattr_with_name<Name: Copy + rustix::path::Arg>(
    path: &Path,
    name: Name,
) -> io::Result<Vec<u8>> {
    let mut empty = [0u8; 0];
    let len = rustix::fs::getxattr(path, name, &mut empty).map_err(io::Error::from)?;
    let mut value = vec![0u8; len];
    let read = rustix::fs::getxattr(path, name, &mut value).map_err(io::Error::from)?;
    value.truncate(read);
    Ok(value)
}
