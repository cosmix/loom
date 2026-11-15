//! Bounded, locked JSONL storage. Domain validation belongs to the sink.
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use super::safe_fs::{safe_create_dir_all, safe_open_append};

pub(crate) const MAX_BYTES: u64 = 1024 * 1024;

/// Append one record to `relative` beneath `root`, refusing a symlink at any
/// component below `root` and `root` itself being one, as `read` and `drain`
/// do. The byte cap is checked under the append lock.
pub(crate) fn append<T: Serialize>(
    root: &Path,
    relative: &str,
    value: &T,
    record_limit: usize,
) -> Result<()> {
    let mut line = serde_json::to_vec(value)?;
    if line.len() > record_limit {
        bail!("Spool record exceeds {record_limit}-byte limit");
    }
    let relpath = Path::new(relative);
    if let Some(parent) = relpath.parent().filter(|p| !p.as_os_str().is_empty()) {
        safe_create_dir_all(root, parent, 0o755)?;
    }
    let mut file = safe_open_append(root, relpath)?;
    if file.metadata()?.len().saturating_add(line.len() as u64 + 1) > MAX_BYTES {
        bail!(
            "Spool {} has reached its {MAX_BYTES}-byte cap",
            root.join(relative).display()
        );
    }
    line.push(b'\n');
    file.write_all(&line)?;
    Ok(())
}

/// Every complete, non-empty line of the JSONL file at `path`, without the
/// newline. The open is `O_NOFOLLOW` and the file must be regular; a missing
/// file is empty. A final part without a terminating newline is a torn write
/// and is skipped. More than `max_bytes` or more than `max_rows` lines is an
/// error. Parsing belongs to the caller.
pub(crate) fn read_complete_lines(
    path: &Path,
    max_bytes: u64,
    max_rows: usize,
) -> Result<Vec<Vec<u8>>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to open {}", path.display()))
        }
    };
    ensure!(
        file.metadata()?.is_file(),
        "{} is not a regular file",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    ensure!(
        bytes.len() as u64 <= max_bytes,
        "{} exceeds its {max_bytes}-byte read cap",
        path.display()
    );
    let mut parts: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    // The part after the last newline is empty, or a torn write.
    parts.pop();
    let lines: Vec<Vec<u8>> = parts
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    ensure!(
        lines.len() <= max_rows,
        "{} exceeds its {max_rows}-row cap",
        path.display()
    );
    Ok(lines)
}

fn contents(file: &File) -> Result<String> {
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        bail!("Spool exceeds {MAX_BYTES}-byte cap");
    }
    Ok(String::from_utf8(bytes)?)
}

pub(crate) fn parse<T: DeserializeOwned>(contents: &str) -> (Vec<T>, usize) {
    let mut values = Vec::new();
    let mut skipped = 0;
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_str(line) {
            Ok(value) => values.push(value),
            Err(_) => skipped += 1,
        }
    }
    (values, skipped)
}

pub(crate) fn read<T: DeserializeOwned>(root: &Path, relative: &str) -> Result<Vec<T>> {
    let Some(file) = super::safe_read::open_regular_no_follow(root, relative, libc::O_RDONLY)?
    else {
        return Ok(Vec::new());
    };
    file.lock_shared()?;
    Ok(parse(&contents(&file)?).0)
}

pub(crate) fn drain<T: DeserializeOwned>(
    root: &Path,
    relative: &str,
    sink: &mut dyn FnMut(&T) -> Result<()>,
) -> Result<(usize, usize)> {
    let Some(file) = super::safe_read::open_regular_no_follow(root, relative, libc::O_RDWR)? else {
        return Ok((0, 0));
    };
    drain_file(file, sink)
}

/// Drain an already-opened spool; open and mutation failures keep delivery retryable.
pub(crate) fn drain_file<T: DeserializeOwned>(
    file: File,
    sink: &mut dyn FnMut(&T) -> Result<()>,
) -> Result<(usize, usize)> {
    file.lock_exclusive()?;
    let (values, skipped) = parse::<T>(&contents(&file)?);
    for value in &values {
        sink(value)?;
    }
    file.set_len(0)?;
    Ok((values.len(), skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_external_oversized_spool_before_sink() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("spool"),
            vec![b' '; MAX_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read::<String>(root.path(), "spool").is_err());
        assert!(drain::<String>(root.path(), "spool", &mut |_| panic!(
            "oversized input reached sink"
        ))
        .is_err());
    }

    #[test]
    fn complete_lines_enforce_byte_and_row_caps_and_treat_missing_as_empty() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("rows.jsonl");
        assert!(read_complete_lines(&path, 64, 2).unwrap().is_empty());
        std::fs::write(&path, "a\nb\nc\n").unwrap();
        assert!(read_complete_lines(&path, 64, 2).is_err());
        assert!(read_complete_lines(&path, 5, 10).is_err());
        assert_eq!(read_complete_lines(&path, 6, 3).unwrap().len(), 3);
    }

    #[test]
    fn append_creates_a_missing_parent_beneath_root() {
        let root = tempfile::tempdir().unwrap();
        append(root.path(), "nested/spool.jsonl", &"x", 64).unwrap();
        let body = std::fs::read_to_string(root.path().join("nested/spool.jsonl")).unwrap();
        assert_eq!(body, "\"x\"\n");
    }
}
