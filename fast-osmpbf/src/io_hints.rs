use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

/// Open a file with sequential read I/O hints.
///
/// On Unix, acquires a shared advisory lock (`LOCK_SH`) to prevent concurrent
/// writers from modifying the file. On Windows, denies write sharing so other
/// processes cannot open the file for writing.
///
/// All hints are best-effort; only the `File::open` error propagates.
pub(crate) fn open_sequential_read_only<P: AsRef<Path>>(path: P) -> io::Result<File> {
    open_sequential_read_only_impl(path.as_ref())
}

#[cfg(windows)]
fn open_sequential_read_only_impl(path: &Path) -> io::Result<File> {
    // FILE_FLAG_SEQUENTIAL_SCAN = 0x0800_0000
    // FILE_SHARE_READ = 0x00000001 — deny write sharing to prevent concurrent modification
    OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001)
        .custom_flags(0x0800_0000)
        .open(path)
}

#[cfg(target_os = "linux")]
fn open_sequential_read_only_impl(path: &Path) -> io::Result<File> {
    use rustix::fs::{Advice, FlockOperation, OFlags, fadvise, flock};
    use rustix::io::Errno;

    // Try O_NOATIME first; on EPERM retry without it.
    let oflags = OFlags::RDONLY | OFlags::NOATIME | OFlags::CLOEXEC;
    let fd = match rustix::fs::open(path, oflags, rustix::fs::Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::PERM) | Err(Errno::ACCESS) => rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?,
        Err(e) => return Err(io::Error::from(e)),
    };

    // Shared advisory lock — prevents concurrent writers that also use flock
    let _ = flock(&fd, FlockOperation::LockShared);

    // Apply fadvise before converting to File
    let _ = fadvise(&fd, 0, None, Advice::Sequential);
    let _ = fadvise(&fd, 0, None, Advice::NoReuse);

    // OwnedFd -> File is safe; File takes ownership of the fd
    Ok(File::from(fd))
}

#[cfg(any(target_os = "freebsd", target_os = "illumos"))]
fn open_sequential_read_only_impl(path: &Path) -> io::Result<File> {
    use rustix::fs::{Advice, FlockOperation, fadvise, flock};

    let file = File::open(path)?;

    // Shared advisory lock — prevents concurrent writers that also use flock
    let _ = flock(&file, FlockOperation::LockShared);
    let _ = fadvise(&file, 0, None, Advice::Sequential);
    let _ = fadvise(&file, 0, None, Advice::NoReuse);

    Ok(file)
}

#[cfg(target_vendor = "apple")]
fn open_sequential_read_only_impl(path: &Path) -> io::Result<File> {
    use rustix::fs::{FlockOperation, fcntl_rdadvise, flock};
    use std::os::unix::fs::MetadataExt;

    let file = File::open(path)?;

    // Shared advisory lock — prevents concurrent writers that also use flock
    let _ = flock(&file, FlockOperation::LockShared);

    let metadata = file.metadata()?;
    let len = metadata.len();

    if len > 0 {
        let _ = fcntl_rdadvise(&file, 0, len);
    }

    Ok(file)
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "illumos",
    target_vendor = "apple",
)))]
fn open_sequential_read_only_impl(path: &Path) -> io::Result<File> {
    // Fallback: plain open, no hints
    File::open(path)
}
