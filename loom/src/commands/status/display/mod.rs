mod runners;
mod sessions;
mod stages;
mod worktrees;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::fs;

pub use runners::{display_runner_health, load_runners};
pub use sessions::display_sessions;
pub use stages::display_stages;
pub use worktrees::display_worktrees;

pub fn count_files(dir: &std::path::Path) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }

    let count = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
        .count();

    Ok(count)
}
