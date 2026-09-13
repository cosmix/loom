use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{ensure, Context, Result};

use super::{ForwardObservation, ForwardReceipt};

const MAX_RECEIPTS_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RECEIPT_LINES: usize = 20_000;

pub fn fold_observations(
    observations: impl IntoIterator<Item = ForwardObservation>,
) -> Vec<ForwardReceipt> {
    let mut receipts: Vec<ForwardReceipt> = Vec::new();
    let mut indices: HashMap<String, usize> = HashMap::new();
    for observation in observations {
        if let Some(index) = indices.get(&observation.receipt_id).copied() {
            receipts[index].apply(observation);
        } else {
            let index = receipts.len();
            indices.insert(observation.receipt_id.clone(), index);
            receipts.push(ForwardReceipt::from_first(observation));
        }
    }
    receipts
}

#[derive(Debug, Default)]
pub struct ReceiptLoad {
    pub receipts: Vec<ForwardReceipt>,
    pub malformed: usize,
    pub truncated: bool,
}

impl ReceiptLoad {
    pub fn get(&self, receipt_id: &str) -> Option<&ForwardReceipt> {
        self.receipts
            .iter()
            .find(|receipt| receipt.receipt_id == receipt_id)
    }

    pub fn for_session<'a>(
        &'a self,
        loom_session_id: &'a str,
    ) -> impl Iterator<Item = &'a ForwardReceipt> + 'a {
        self.receipts
            .iter()
            .filter(move |receipt| receipt.identity.loom_session_id == loom_session_id)
    }
}

pub fn load_receipts(path: &Path) -> Result<ReceiptLoad> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReceiptLoad::default())
        }
        Err(error) => return Err(error).context("failed to open forward receipts"),
    };
    let metadata = file
        .metadata()
        .context("failed to inspect forward receipts")?;
    ensure!(
        metadata.is_file(),
        "forward receipts path is not a regular file"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_RECEIPTS_BYTES)
        .read_to_end(&mut bytes)
        .context("failed to read forward receipts")?;
    Ok(decode_loaded(&bytes, metadata.len() > MAX_RECEIPTS_BYTES))
}

fn decode_loaded(bytes: &[u8], mut truncated: bool) -> ReceiptLoad {
    let mut observations = Vec::new();
    let mut malformed = 0;
    let mut lines = bytes.split(|byte| *byte == b'\n').peekable();
    let mut count = 0;
    while let Some(line) = lines.next() {
        if line.is_empty() && lines.peek().is_none() {
            break;
        }
        if count == MAX_RECEIPT_LINES {
            truncated = true;
            break;
        }
        count += 1;
        match std::str::from_utf8(line)
            .ok()
            .and_then(|line| ForwardObservation::decode_line(line).ok())
        {
            Some(observation) => observations.push(observation),
            None => malformed += 1,
        }
    }
    ReceiptLoad {
        receipts: fold_observations(observations),
        malformed,
        truncated,
    }
}
