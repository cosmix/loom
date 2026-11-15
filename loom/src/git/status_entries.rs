//! Strict parsing of `git status --porcelain=v1 -z` listings.

use anyhow::{ensure, Context};

/// `git status` arguments for a NUL-separated porcelain v1 listing that names
/// every untracked file rather than collapsing untracked directories.
pub(crate) const PORCELAIN_V1_Z: [&str; 4] =
    ["status", "--porcelain=v1", "-z", "--untracked-files=all"];

/// One record of a porcelain v1 listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusEntry {
    /// The `XY` status bytes: index column, then work-tree column.
    pub code: [u8; 2],
    /// The path as git names it (a rename's or copy's destination).
    pub path: String,
    /// The source path of a rename or copy.
    pub origin: Option<String>,
}

impl StatusEntry {
    /// The index column (`X`).
    pub(crate) fn index(&self) -> u8 {
        self.code[0]
    }

    /// The work-tree column (`Y`).
    pub(crate) fn worktree(&self) -> u8 {
        self.code[1]
    }

    /// Whether git lists the path as untracked (`??`).
    pub(crate) fn is_untracked(&self) -> bool {
        self.code == *b"??"
    }

    /// The two status bytes as text.
    pub(crate) fn code_str(&self) -> &str {
        // Parsing admits only ASCII status bytes.
        std::str::from_utf8(&self.code).expect("status code bytes are ASCII")
    }
}

/// Parse the stdout of `git status --porcelain=v1 -z`.
///
/// The whole listing must be UTF-8 (a lossy decode could name a different
/// file) and, when non-empty, end in NUL. Each record is `XY <path>` with
/// both columns in `[ MADRCUT?!]` and not both blank; an `R` or `C` in either
/// column takes the next NUL field as the origin. Records are never trimmed: a
/// leading space is the index column.
pub(crate) fn parse_porcelain_v1_z(output: &[u8]) -> anyhow::Result<Vec<StatusEntry>> {
    let output = std::str::from_utf8(output).context("git status contains a non-UTF-8 path")?;
    ensure!(
        output.is_empty() || output.ends_with('\0'),
        "git status record lacks a terminating NUL"
    );
    let mut entries = Vec::new();
    let mut records = output.split_terminator('\0');
    while let Some(record) = records.next() {
        let (code, path) = parse_record(record)?;
        let origin = if code.iter().any(|byte| matches!(byte, b'R' | b'C')) {
            // Porcelain -z emits the destination first, then the source alone.
            let source = records
                .next()
                .context("git status rename/copy lacks a source")?;
            ensure!(!source.is_empty(), "git status rename/copy source is empty");
            Some(source.to_owned())
        } else {
            None
        };
        entries.push(StatusEntry {
            code,
            path: path.to_owned(),
            origin,
        });
    }
    Ok(entries)
}

fn parse_record(record: &str) -> anyhow::Result<([u8; 2], &str)> {
    let code: [u8; 2] = record
        .as_bytes()
        .get(..2)
        .and_then(|bytes| bytes.try_into().ok())
        .context("git status record lacks an XY status")?;
    ensure!(
        code != *b"  "
            && code.iter().all(|byte| {
                matches!(
                    byte,
                    b' ' | b'M' | b'A' | b'D' | b'R' | b'C' | b'U' | b'T' | b'?' | b'!'
                )
            })
            && record.as_bytes().get(2) == Some(&b' '),
        "invalid git status record header"
    );
    let path = record.get(3..).context("git status record lacks a path")?;
    ensure!(!path.is_empty(), "git status path is empty");
    Ok((code, path))
}

#[cfg(test)]
#[path = "status_entries_tests.rs"]
mod tests;
