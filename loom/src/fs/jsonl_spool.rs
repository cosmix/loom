//! Bounded, locked JSONL storage. Domain validation belongs to the sink.
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub(crate) const MAX_BYTES: u64 = 1024 * 1024;

pub(crate) fn append<T: Serialize>(
    root: &Path,
    relative: &str,
    value: &T,
    record_limit: usize,
) -> Result<()> {
    let line = serde_json::to_string(value)?;
    if line.len() > record_limit {
        bail!("Spool record exceeds {record_limit}-byte limit");
    }
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().context("Missing spool parent")?)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.lock_exclusive()?;
    if file.metadata()?.len().saturating_add(line.len() as u64 + 1) > MAX_BYTES {
        bail!(
            "Spool {} has reached its {MAX_BYTES}-byte cap",
            path.display()
        );
    }
    writeln!(file, "{line}")?;
    Ok(())
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
}
