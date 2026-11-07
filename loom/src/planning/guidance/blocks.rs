//! Persistent launch blocks for changes to session configuration.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::fs::safe_fs::{open_safely, safe_remove_in_workdir, safe_write_with_mode_in_workdir};
use crate::planning::runtime::{open_runtime_dir, RuntimePaths};

const BLOCK_FILE: &str = "config-block.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigBlock {
    pub reason: String,
    pub changed: Vec<String>,
    pub set_at: String,
}

pub struct StartBlocks;

impl StartBlocks {
    /// Persist a block in the server's runtime directory, replacing any prior block.
    pub fn block(runtime: &RuntimePaths, reason: &str, changed: &[String]) -> Result<()> {
        let block = ConfigBlock {
            reason: reason.to_owned(),
            changed: changed.to_vec(),
            set_at: Utc::now().to_rfc3339(),
        };
        let content = serde_json::to_vec(&block).context("cannot serialize config block")?;
        let dirfd = open_runtime_dir(runtime, true)?;
        safe_write_with_mode_in_workdir(dirfd.as_raw_fd(), Path::new(BLOCK_FILE), &content, 0o600)
            .with_context(|| format!("cannot write {}", runtime.dir.join(BLOCK_FILE).display()))
    }

    /// Remove the block; an absent runtime directory or block is already clear.
    pub fn clear(runtime: &RuntimePaths) -> Result<()> {
        let dirfd = match open_runtime_dir(runtime, false) {
            Ok(dirfd) => dirfd,
            Err(error) if is_missing(&error) => return Ok(()),
            Err(error) => return Err(error),
        };
        safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new(BLOCK_FILE))
            .with_context(|| format!("cannot remove {}", runtime.dir.join(BLOCK_FILE).display()))
    }

    /// Only absence clears a block; unreadable or malformed state fails closed.
    pub fn current(runtime: &RuntimePaths) -> Result<Option<ConfigBlock>> {
        let dirfd = match open_runtime_dir(runtime, false) {
            Ok(dirfd) => dirfd,
            Err(error) if is_missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let path = runtime.dir.join(BLOCK_FILE);
        // Nonblocking open also lets us reject a FIFO without waiting for a writer.
        let fd = match open_safely(
            dirfd.as_raw_fd(),
            Path::new(BLOCK_FILE),
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        ) {
            Ok(fd) => fd,
            Err(error) if is_missing(&error) => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()));
            }
        };
        let file = File::from(fd);
        let metadata = file
            .metadata()
            .with_context(|| format!("cannot inspect {}", path.display()))?;
        ensure!(
            metadata.is_file(),
            "{} is not a regular file",
            path.display()
        );
        serde_json::from_reader(file)
            .with_context(|| format!("cannot parse config block {}", path.display()))
            .map(Some)
    }
}

fn is_missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}
