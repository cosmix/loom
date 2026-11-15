use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::fs::jsonl_spool::read_complete_lines;
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
    if missing_is_error {
        // Surfaces a missing ledger as NotFound, which the shared reader maps to empty.
        std::fs::symlink_metadata(path)?;
    }
    let lines = read_complete_lines(path, max_bytes, max_rows).context("reading JSONL ledger")?;
    Ok(lines
        .iter()
        .map(|line| serde_json::from_slice(line).map_err(|error| error.to_string()))
        .collect())
}
