//! Owned private storage. Unix directory handles anchor entry opens/atomic writes.
//! The configured parent hierarchy is trusted; leaf directories/files are no-follow.
use anyhow::{Context, Result, bail, ensure};
#[cfg(unix)]
use rustix::fs::{AtFlags, Mode, OFlags};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

#[cfg(unix)]
fn owned(file: &File, private: bool, directory: bool) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let m = file.metadata()?;
    ensure!(
        m.uid() == rustix::process::geteuid().as_raw(),
        "Private storage has unexpected ownership"
    );
    ensure!(
        m.permissions().mode() & 0o022 == 0,
        "Private storage must not be writable by group/other users"
    );
    ensure!(
        if directory { m.is_dir() } else { m.is_file() },
        "Private storage has unexpected file type"
    );
    ensure!(
        directory || m.nlink() == 1,
        "Private files must not be hard-linked"
    );
    ensure!(
        !private || m.permissions().mode() & 0o077 == 0,
        "Private storage permissions must exclude group/other access"
    );
    Ok(())
}
#[cfg(unix)]
pub(crate) fn directory(path: &Path, create: bool) -> Result<File> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let opened = rustix::fs::open(path, flags, Mode::empty());
    let fd = match opened {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) if create => {
            let parent = path.parent().context("Missing private directory parent")?;
            let name = path.file_name().context("Missing private directory name")?;
            if !parent.exists() {
                directory(parent, true)?;
            }
            let p: File = rustix::fs::open(parent, flags, Mode::empty())
                .context("Private directory parent must not be a symlink")?
                .into();
            owned(&p, false, true)?;
            match rustix::fs::mkdirat(&p, name, Mode::from_bits_truncate(0o700)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(_) => bail!("Cannot create private directory"),
            }
            rustix::fs::openat(&p, name, flags, Mode::empty())
                .context("Cannot open private directory")?
        }
        Err(_) => bail!("Private directory must be a real owned directory"),
    };
    let file: File = fd.into();
    owned(&file, false, true)?;
    if create {
        rustix::fs::fchmod(&file, Mode::from_bits_truncate(0o700))?;
    }
    owned(&file, true, true)?;
    Ok(file)
}
#[cfg(not(unix))]
pub(crate) fn directory(_path: &Path, _create: bool) -> Result<File> {
    bail!("Private storage is currently supported on Linux/macOS only");
}

#[cfg(unix)]
fn entry(dir: &File, name: &std::ffi::OsStr) -> Result<File> {
    let file: File = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .context("Cannot open private file")?
    .into();
    owned(&file, true, false)?;
    Ok(file)
}
pub(crate) fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let dir = directory(path.parent().context("Missing private parent")?, false)?;
    #[cfg(unix)]
    let file = entry(&dir, path.file_name().context("Missing private filename")?)?;
    #[cfg(not(unix))]
    let file: File = {
        drop(dir);
        bail!("Private storage unsupported")
    };
    ensure!(
        file.metadata()?.len() <= limit as u64,
        "Private file exceeds size limit"
    );
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "Private file exceeds size limit");
    Ok(bytes)
}
pub(crate) fn valid(path: &Path, limit: usize) -> bool {
    #[cfg(unix)]
    {
        let Ok(dir) = directory(path.parent().unwrap_or(Path::new("")), false) else {
            return false;
        };
        path.file_name()
            .and_then(|name| entry(&dir, name).ok())
            .and_then(|file| file.metadata().ok())
            .is_some_and(|m| m.len() <= limit as u64)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, limit);
        false
    }
}
#[cfg(unix)]
pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let dir = directory(path.parent().context("Missing private parent")?, true)?;
    let name = path.file_name().context("Missing private filename")?;
    match rustix::fs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(m) => ensure!(
            m.st_uid == rustix::process::geteuid().as_raw()
                && m.st_nlink == 1
                && m.st_mode & 0o170000 == 0o100000
                && m.st_mode & 0o077 == 0,
            "Existing private file must be owned, regular and private"
        ),
        Err(rustix::io::Errno::NOENT) => {}
        Err(_) => bail!("Cannot validate private destination"),
    }
    let temporary = format!(
        ".jirust-write-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let fd = rustix::fs::openat(
        &dir,
        &temporary,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .context("Cannot create private temporary file")?;
    let result = (|| -> Result<()> {
        let mut file: File = fd.into();
        file.write_all(bytes)?;
        file.sync_all()?;
        rustix::fs::renameat(&dir, &temporary, &dir, name)?;
        dir.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = rustix::fs::unlinkat(&dir, &temporary, AtFlags::empty());
    }
    result
}
#[cfg(not(unix))]
pub(crate) fn write(_path: &Path, _bytes: &[u8]) -> Result<()> {
    bail!("Private storage unsupported")
}

/// Nonblocking advisory lock: concurrent client processes must not race quotas.
pub(crate) fn lock(path: &Path) -> Result<File> {
    let dir = directory(path, true)?;
    #[cfg(unix)]
    {
        // CLOEXEC descriptors can briefly remain inherited between fork and exec
        // in another thread. Retry that window, but never wait on a stale writer.
        for attempt in 0..=20 {
            match rustix::fs::flock(&dir, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(dir),
                Err(rustix::io::Errno::WOULDBLOCK) if attempt < 20 => {
                    std::thread::sleep(std::time::Duration::from_millis(1))
                }
                Err(_) => bail!("Private cache busy; existing data retained"),
            }
        }
        Err(anyhow::anyhow!(
            "Private cache busy; existing data retained"
        ))
    }
    #[cfg(not(unix))]
    {
        Ok(dir)
    }
}

pub(crate) fn quota(
    path: &Path,
    destination: &Path,
    new_size: usize,
    max_bytes: u64,
    max_entries: usize,
) -> Result<()> {
    let dir = directory(path, true)?;
    let mut bytes = new_size as u64;
    let mut entries = 1usize;
    for item in fs::read_dir(path)? {
        let item = item?;
        let name = item.file_name();
        let Some(s) = name.to_str() else {
            continue;
        };
        if s.len() != 69
            || !s.ends_with(".json")
            || !s[..64].bytes().all(|c| c.is_ascii_hexdigit())
            || name == destination.file_name().unwrap_or_default()
        {
            continue;
        }
        #[cfg(unix)]
        let file = entry(&dir, &name)?;
        #[cfg(not(unix))]
        let file: File = {
            let _ = &dir;
            bail!("Private storage unsupported")
        };
        bytes = bytes.saturating_add(file.metadata()?.len());
        entries += 1;
        ensure!(
            bytes <= max_bytes && entries <= max_entries,
            "Ticket cache quota reached; remove old cached data explicitly"
        );
    }
    ensure!(
        bytes <= max_bytes && entries <= max_entries,
        "Ticket cache quota reached; remove old cached data explicitly"
    );
    Ok(())
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn no_follow_private_reads_and_writes_are_bounded() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("owned.json");
        write(&p, b"fixture").unwrap();
        assert_eq!(read(&p, 7).unwrap(), b"fixture");
        assert!(read(&p, 6).is_err());
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read(&p, 7).is_err());
        assert!(write(&p, b"replacement").is_err());
        fs::remove_file(&p).unwrap();
        let outside = d.path().join("outside");
        fs::write(&outside, b"external-fixture").unwrap();
        symlink(&outside, &p).unwrap();
        assert!(read(&p, 100).is_err());
        assert!(write(&p, b"replacement").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"external-fixture");
        let link = d.path().join("link");
        symlink(d.path(), &link).unwrap();
        assert!(directory(&link, true).is_err());
    }
    #[test]
    fn hardlinks_and_concurrent_writer_locks_fail_closed() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        let b = d.path().join("b");
        write(&a, b"fixture").unwrap();
        fs::hard_link(&a, &b).unwrap();
        assert!(read(&a, 100).is_err());
        assert!(read(&b, 100).is_err());
        let held = lock(d.path()).unwrap();
        assert!(lock(d.path()).is_err());
        drop(held);
        assert!(lock(d.path()).is_ok());
    }
    #[test]
    fn aggregate_quota_preserves_existing_data_and_allows_replacement() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join(format!("{}.json", "a".repeat(64)));
        write(&a, b"1234").unwrap();
        let b = d.path().join(format!("{}.json", "b".repeat(64)));
        assert!(quota(d.path(), &b, 7, 10, 2).is_err());
        assert!(quota(d.path(), &b, 1, 10, 1).is_err());
        assert!(quota(d.path(), &a, 5, 10, 1).is_ok());
        assert_eq!(read(&a, 10).unwrap(), b"1234");
        assert!(quota(d.path(), &b, 6, 10, 2).is_ok());
    }
    #[test]
    fn special_files_and_sparse_oversize_entries_are_not_read() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sparse");
        write(&p, b"").unwrap();
        File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_len(1024 * 1024)
            .unwrap();
        assert!(read(&p, 1024).is_err());
        let fifo = d.path().join("fifo");
        assert!(
            std::process::Command::new("/usr/bin/mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        fs::set_permissions(&fifo, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read(&fifo, 1024).is_err());
    }
}
