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
        let path = journal.with_extension("jsonl.lock");
        let pid = std::process::id();
        let claim = create_claim(&path, pid)?;
        for _ in 0..RETRIES {
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return claim_lock(path, claim, pid),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    recover_abandoned(&path)?;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    remove_claim(&claim);
                    return Err(error).context("creating lifecycle journal lock");
                }
            }
        }
        remove_claim(&claim);
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

fn claim_lock(path: PathBuf, claim: PathBuf, pid: u32) -> Result<JournalLock> {
    let owner = path.join("owner");
    if let Err(error) = fs::hard_link(&claim, &owner) {
        remove_claim(&claim);
        let _ = fs::remove_dir(&path);
        return Err(error).context("publishing lifecycle journal lock owner");
    }
    remove_claim(&claim);
    Ok(JournalLock { path, pid })
}

fn create_claim(lock: &Path, pid: u32) -> Result<PathBuf> {
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
                write!(file, "pid={pid}\ncreated={created}\n")?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("creating lifecycle lock claim"),
        }
    }
    bail!("could not create unique lifecycle lock claim")
}

fn recover_abandoned(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
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
            fs::remove_dir(path).context("reclaiming empty lifecycle lock")?;
            return Ok(());
        }
    };
    if now.saturating_sub(owner.created) < STALE_AFTER_SECS || pid_is_alive(owner.pid) {
        return Ok(());
    }
    fs::remove_file(owner_path).context("removing abandoned lifecycle lock owner")?;
    fs::remove_dir(path).context("removing abandoned lifecycle lock")?;
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
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
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

fn remove_claim(path: &Path) {
    let _ = fs::remove_file(path);
}
