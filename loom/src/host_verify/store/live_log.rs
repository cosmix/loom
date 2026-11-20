use super::*;
use std::collections::VecDeque;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;

use nix::errno::Errno;
use nix::fcntl::{openat, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::{unlinkat, UnlinkatFlags};

use crate::fs::safe_replace::open_parent_at;

/// Bytes of a cut log kept from the start of the output.
const HEAD_BYTES: usize = MAX_LOG_BYTES / 4;
/// Most recent bytes of the output a cut log keeps after the truncation line
/// when it is rewritten. It then grows with new output up to
/// [`MAX_LOG_BYTES`], is rewritten again, and so always ends with the latest
/// output.
const TAIL_BYTES: usize = MAX_LOG_BYTES / 4;

/// A criterion's log while it runs: `<job dir>/<label>.log`, written as the
/// output arrives so a hung criterion shows how far it got. It holds at most
/// [`MAX_LOG_BYTES`]: past them, the first [`HEAD_BYTES`] of the output, the
/// truncation line, and the most recent output. The criterion's end replaces
/// it with [`save_criterion_log`]'s copy of the captured output.
pub(crate) struct LiveLog {
    file: File,
    /// The file's length.
    len: usize,
    cut: bool,
    /// The last [`TAIL_BYTES`] of the output.
    recent: VecDeque<u8>,
}

/// Create `<job dir>/<label>.log` afresh for a running criterion. The host
/// child that writes it runs outside the sandbox, so it never writes through
/// an entry it did not create: whatever has the name (a hard link to another
/// file, a symlink, a FIFO) is unlinked, and the create refuses an entry that
/// appears in between.
pub(crate) fn open_live_log(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    label: &str,
) -> Result<LiveLog> {
    let name = criterion_log_name(label)?;
    let rel = job_rel(stage_id, job_id)?.join(&name);
    let root = open_root(work_dir)?;
    let file = open_parent_at(root.as_fd(), &rel)
        .and_then(|dir| create_fresh(&dir, &name))
        .with_context(|| format!("cannot create {}", rel.display()))?;
    Ok(LiveLog {
        file,
        len: 0,
        cut: false,
        recent: VecDeque::new(),
    })
}

/// `name` in `dir` as a new, empty regular file of this process.
fn create_fresh(dir: &OwnedFd, name: &str) -> Result<File> {
    match unlinkat(dir, name, UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(Errno::ENOENT) => {}
        Err(errno) => return Err(errno).context("cannot remove the earlier entry"),
    }
    let flags =
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let mode = Mode::from_bits_truncate(FILE_MODE as libc::mode_t);
    let fd = openat(dir, name, flags, mode)?;
    Ok(File::from(fd))
}

impl LiveLog {
    /// Keep the last [`TAIL_BYTES`] of the output, `bytes` the newest.
    fn remember(&mut self, bytes: &[u8]) {
        let newest = &bytes[bytes.len().saturating_sub(TAIL_BYTES)..];
        let overflow = (self.recent.len() + newest.len()).saturating_sub(TAIL_BYTES);
        self.recent.drain(..overflow);
        self.recent.extend(newest);
    }

    /// Rewrite the file as its head, the truncation line and the most recent
    /// output; `bytes`, the write that crossed the cap, completes a head the
    /// file does not hold yet.
    fn cut_to_head_and_tail(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if !self.cut {
            if let Some(missing) = HEAD_BYTES.checked_sub(self.len) {
                self.file.write_all_at(&bytes[..missing], self.len as u64)?;
            }
            self.file.write_all_at(TRUNCATION_LINE, HEAD_BYTES as u64)?;
            self.cut = true;
        }
        let start = HEAD_BYTES + TRUNCATION_LINE.len();
        self.file
            .write_all_at(self.recent.make_contiguous(), start as u64)?;
        self.len = start + self.recent.len();
        self.file.set_len(self.len as u64)
    }
}

impl Write for LiveLog {
    /// Takes every byte; past the cap the file is cut to its head and the
    /// most recent output.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.remember(bytes);
        if self.len + bytes.len() <= MAX_LOG_BYTES {
            self.file.write_all_at(bytes, self.len as u64)?;
            self.len += bytes.len();
        } else {
            self.cut_to_head_and_tail(bytes)?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// The criterion logs of job `job_id` whose label has no outcome among
/// `outcomes`, by name: the log of a criterion that runs now, or that was
/// running when the job stopped. None when the job directory is gone.
pub(crate) fn unfinished_logs(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    outcomes: &[CriterionOutcome],
) -> Result<Vec<String>> {
    let rel = job_rel(stage_id, job_id)?;
    let entries = match list_dir_no_follow(&open_root(work_dir)?, &rel) {
        Ok(entries) => entries,
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error.context(format!("cannot list {}", rel.display()))),
    };
    let unfinished = |name: &String| {
        name.strip_suffix(".log").is_some_and(|label| {
            criterion_log_name(label).is_ok() && !outcomes.iter().any(|o| o.label == label)
        })
    };
    let mut logs: Vec<String> = entries
        .into_iter()
        .filter(|(_, kind)| *kind == EntryKind::File)
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(unfinished)
        .collect();
    logs.sort();
    Ok(logs)
}
