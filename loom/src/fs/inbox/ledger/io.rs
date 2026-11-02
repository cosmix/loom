//! Locked ledger reads and recovery on the same validated file descriptor.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{bail, Context, Result};
use fs2::FileExt;

use super::{LedgerRecord, MAX_LEDGER_LINE_BYTES};
use crate::fs::inbox::paths::{inbox_root_relpath, ledger_relpath, session_relpath};
use crate::fs::safe_fs::{open_safely, safe_create_dir_all_in_workdir, safe_open_dirfd};
use crate::fs::safe_read::{is_not_found, single_link_regular_file};

struct Contents {
    records: Vec<LedgerRecord>,
    valid_end: u64,
    torn: bool,
    needs_newline: bool,
}

fn open(work_dir: &Path, session_id: &str, write: bool) -> Result<Option<File>> {
    let root = match safe_open_dirfd(work_dir) {
        Ok(root) => root,
        Err(error) if !write && is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    if write {
        safe_create_dir_all_in_workdir(root.as_raw_fd(), &inbox_root_relpath(), 0o700)?;
        safe_create_dir_all_in_workdir(root.as_raw_fd(), &session_relpath(session_id)?, 0o700)?;
    }
    let flags = if write {
        libc::O_RDWR | libc::O_CREAT
    } else {
        libc::O_RDONLY
    };
    let fd = match open_safely(
        root.as_raw_fd(),
        &ledger_relpath(session_id)?,
        flags | libc::O_NONBLOCK,
        if write { 0o600 } else { 0 },
    ) {
        Ok(fd) => fd,
        Err(error) if !write && is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let file = File::from(fd);
    single_link_regular_file(&file)?;
    Ok(Some(file))
}

pub(super) fn read(work_dir: &Path, session_id: &str) -> Result<Vec<LedgerRecord>> {
    let Some(mut file) = open(work_dir, session_id, false)? else {
        return Ok(Vec::new());
    };
    FileExt::lock_shared(&file).context("locking inbox ledger for reading")?;
    Ok(scan(&mut file)?.records)
}

pub(super) fn append(work_dir: &Path, session_id: &str, line: &[u8]) -> Result<()> {
    let mut file = open(work_dir, session_id, true)?.expect("write creates ledger");
    FileExt::lock_exclusive(&file).context("locking inbox ledger for writing")?;
    let contents = scan(&mut file)?;
    if contents.torn {
        file.set_len(contents.valid_end)
            .context("truncating torn ledger suffix")?;
    }
    file.seek(SeekFrom::Start(contents.valid_end))?;
    if contents.needs_newline {
        file.write_all(b"\n")?;
    }
    file.write_all(line).context("appending inbox ledger")?;
    file.sync_all().context("syncing inbox ledger")
}

// Keep all IDs for the lifetime of the session: replay can remain possible.
// Bound each line instead of bounding the whole append-only history.
fn scan(file: &mut File) -> Result<Contents> {
    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file);
    let mut contents = Contents {
        records: Vec::new(),
        valid_end: 0,
        torn: false,
        needs_newline: false,
    };
    loop {
        let mut line = Vec::new();
        let bytes = (&mut reader)
            .take((MAX_LEDGER_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if bytes == 0 {
            break;
        }
        if bytes > MAX_LEDGER_LINE_BYTES {
            bail!("ledger line exceeds {MAX_LEDGER_LINE_BYTES} bytes");
        }
        let valid = match std::str::from_utf8(&line) {
            Ok(text) if text.trim().is_empty() => true,
            Ok(text) => match serde_json::from_str::<LedgerRecord>(text) {
                Ok(record) => {
                    contents.records.push(record);
                    true
                }
                Err(_) => false,
            },
            Err(_) => false,
        };
        if !valid {
            if !reader.fill_buf()?.is_empty() {
                bail!("interior ledger line did not parse as JSON");
            }
            contents.torn = true;
            break;
        }
        contents.valid_end += bytes as u64;
        contents.needs_newline = !line.ends_with(b"\n");
    }
    Ok(contents)
}
