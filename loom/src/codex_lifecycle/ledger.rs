use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use serde_json::Value;

use crate::subagent_lifecycle::LifecycleRecord;

use super::CodexAuthorization;

const MAX_AUTHORIZATION_BYTES: u64 = 1024 * 1024;
const MAX_AUTHORIZATION_ROWS: usize = 256;
const MAX_LIFECYCLE_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) enum LedgerRow {
    Authorization(Box<CodexAuthorization>),
    Invalid(String),
    Legacy,
}

pub(crate) fn read_authorization_rows(path: &Path) -> Result<Vec<LedgerRow>> {
    let values = read_json_lines(path, MAX_AUTHORIZATION_BYTES, MAX_AUTHORIZATION_ROWS, false)?;
    Ok(values
        .into_iter()
        .map(|value| match value {
            Ok(value) if value.get("v").and_then(Value::as_u64) == Some(2) => {
                CodexAuthorization::from_v2_value(&value)
                    .map(Box::new)
                    .map(LedgerRow::Authorization)
                    .unwrap_or_else(|error| LedgerRow::Invalid(error.to_string()))
            }
            Ok(_) => LedgerRow::Legacy,
            Err(error) => LedgerRow::Invalid(error),
        })
        .collect())
}

pub(crate) fn read_lifecycle_records(path: &Path) -> Result<Vec<LifecycleRecord>> {
    read_json_lines(path, MAX_LIFECYCLE_BYTES, MAX_AUTHORIZATION_ROWS * 4, true)?
        .into_iter()
        .map(|value| {
            let value = value.map_err(anyhow::Error::msg)?;
            serde_json::from_value(value).context("invalid lifecycle record")
        })
        .collect()
}

fn read_json_lines(
    path: &Path,
    max_bytes: u64,
    max_rows: usize,
    missing_is_error: bool,
) -> Result<Vec<std::result::Result<Value, String>>> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !missing_is_error => {
            return Ok(Vec::new())
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        file.metadata()?.is_file(),
        "JSONL ledger is not a regular file"
    );
    let mut bytes = Vec::new();
    file.by_ref().take(max_bytes + 1).read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= max_bytes,
        "JSONL ledger exceeds read cap"
    );
    let terminated = bytes.last() == Some(&b'\n');
    let parts: Vec<_> = bytes.split(|byte| *byte == b'\n').collect();
    let mut rows = Vec::new();
    for (index, line) in parts.iter().enumerate() {
        let final_part = index + 1 == parts.len();
        if line.is_empty() || (!terminated && final_part) {
            continue;
        }
        ensure!(rows.len() < max_rows, "JSONL ledger exceeds row cap");
        rows.push(serde_json::from_slice(line).map_err(|error| error.to_string()));
    }
    Ok(rows)
}
