use anyhow::{bail, Context, Result};
use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STALE_AFTER_SECS: u64 = 30;
const RETRIES: usize = 200;

pub(super) struct JournalLock {
    path: PathBuf,
    pid: u32,
}

impl JournalLock {
    pub(super) fn acquire(journal: &Path) -> Result<Self> {
        Self::acquire_with(journal, |_| {})
    }

    fn acquire_with(journal: &Path, mut on_contention: impl FnMut(&Path)) -> Result<Self> {
        let path = journal.with_extension("jsonl.lock");
        let pid = std::process::id();
        let claim = create_claim(&path, pid)?;
        for _ in 0..RETRIES {
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return claim_lock(path, claim, pid),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    on_contention(&path);
                    recover_abandoned(&path)?;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error).context("creating lifecycle journal lock"),
            }
        }
        bail!("timed out acquiring lifecycle journal lock")
    }
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        let owner = self.path.join("owner");
        let owned = read_owner(&owner)
            .ok()
            .flatten()
            .is_some_and(|value| value.pid == self.pid);
        if owned {
            let _ = fs::remove_file(owner);
            let _ = fs::remove_dir(&self.path);
        }
    }
}

struct Owner {
    pid: u32,
    created: u64,
}

struct Claim(PathBuf);

impl Drop for Claim {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn claim_lock(path: PathBuf, claim: Claim, pid: u32) -> Result<JournalLock> {
    let owner = path.join("owner");
    if let Err(error) = fs::hard_link(&claim.0, &owner) {
        let _ = fs::remove_dir(&path);
        return Err(error).context("publishing lifecycle journal lock owner");
    }
    drop(claim);
    Ok(JournalLock { path, pid })
}

fn create_claim(lock: &Path, pid: u32) -> Result<Claim> {
    create_claim_with(lock, pid, |file, pid, created| {
        write!(file, "pid={pid}\ncreated={created}\n")?;
        file.sync_all()?;
        Ok(())
    })
}

fn create_claim_with(
    lock: &Path,
    pid: u32,
    mut initialize: impl FnMut(&mut fs::File, u32, u64) -> Result<()>,
) -> Result<Claim> {
    let parent = lock
        .parent()
        .context("lifecycle journal lock has no parent")?;
    let stem = lock
        .file_name()
        .and_then(|value| value.to_str())
        .context("lifecycle journal lock is not UTF-8")?;
    let created = unix_now()?;
    for nonce in 0..16_u8 {
        let path = parent.join(format!("{stem}.claim.{pid}.{created}.{nonce}"));
        let opened = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path);
        match opened {
            Ok(mut file) => {
                // Own the new file before either initialization step can fail.
                let claim = Claim(path);
                initialize(&mut file, pid, created)?;
                return Ok(claim);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("creating lifecycle lock claim"),
        }
    }
    bail!("could not create unique lifecycle lock claim")
}

fn recover_abandoned(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("lifecycle lock path is not a plain directory")
    }
    let now = unix_now()?;
    let owner_path = path.join("owner");
    let owner = match read_owner(&owner_path)? {
        Some(owner) => owner,
        None => {
            let created = metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .context("lifecycle lock predates Unix epoch")?
                .as_secs();
            if now.saturating_sub(created) < STALE_AFTER_SECS {
                return Ok(());
            }
            match fs::remove_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("reclaiming empty lifecycle lock"),
            }
            return Ok(());
        }
    };
    if now.saturating_sub(owner.created) < STALE_AFTER_SECS || pid_is_alive(owner.pid) {
        return Ok(());
    }
    match fs::remove_file(owner_path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("removing abandoned lifecycle lock owner"),
    }
    match fs::remove_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("removing abandoned lifecycle lock"),
    }
    Ok(())
}

fn read_owner(path: &Path) -> Result<Option<Owner>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("lifecycle lock owner is not a plain file")
    }
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut content = String::new();
    Read::by_ref(&mut file)
        .take(128)
        .read_to_string(&mut content)?;
    let mut lines = content.lines();
    let pid = parse_field(lines.next(), "pid=")?;
    let created = parse_field(lines.next(), "created=")?;
    if lines.next().is_some() {
        bail!("lifecycle lock owner has extra fields")
    }
    Ok(Some(Owner { pid, created }))
}

fn parse_field<T>(line: Option<&str>, prefix: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    line.and_then(|value| value.strip_prefix(prefix))
        .context("lifecycle lock owner field is missing")?
        .parse()
        .context("lifecycle lock owner field is malformed")
}

fn pid_is_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return true;
    };
    !matches!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH))
}

fn unix_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim_paths(parent: &Path) -> Result<Vec<PathBuf>> {
        Ok(fs::read_dir(parent)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|path| path.to_string_lossy().contains(".claim."))
            .collect())
    }

    #[test]
    fn vanished_contended_lock_retries_without_leaking_claim() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let journal = temp.path().join("lifecycle.jsonl");
        let lock_path = journal.with_extension("jsonl.lock");
        fs::create_dir(&lock_path)?;

        let mut contended = false;
        let lock = JournalLock::acquire_with(&journal, |path| {
            assert_eq!(path, lock_path);
            fs::remove_dir(path).expect("old owner released its empty lock");
            contended = true;
        })?;

        assert!(contended);
        assert!(lock_path.join("owner").is_file());
        assert!(claim_paths(temp.path())?.is_empty());
        drop(lock);
        assert!(!lock_path.exists());
        Ok(())
    }

    #[test]
    fn malformed_owner_refusal_removes_only_our_claim() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let journal = temp.path().join("lifecycle.jsonl");
        let lock_path = journal.with_extension("jsonl.lock");
        fs::create_dir(&lock_path)?;
        fs::write(lock_path.join("owner"), "invalid owner")?;

        assert!(JournalLock::acquire(&journal).is_err());
        assert_eq!(
            fs::read_to_string(lock_path.join("owner"))?,
            "invalid owner"
        );
        assert!(claim_paths(temp.path())?.is_empty());
        Ok(())
    }

    #[test]
    fn claim_initialization_errors_remove_new_files() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let lock_path = temp.path().join("lifecycle.jsonl.lock");
        let pid = std::process::id();

        let write_error = create_claim_with(&lock_path, pid, |_, _, _| {
            bail!("injected claim write failure")
        });
        assert!(write_error.is_err());
        assert!(claim_paths(temp.path())?.is_empty());

        let sync_error = create_claim_with(&lock_path, pid, |file, pid, created| {
            write!(file, "pid={pid}\ncreated={created}\n")?;
            bail!("injected claim sync failure")
        });
        assert!(sync_error.is_err());
        assert!(claim_paths(temp.path())?.is_empty());
        Ok(())
    }
}
