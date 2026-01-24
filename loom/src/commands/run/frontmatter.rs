//! YAML frontmatter parsing for stage files.
//!
//! This module re-exports shared stage loading functionality from crate::fs::stage_loading.

// Re-export the shared implementation
#[allow(unused_imports)]
pub use crate::fs::stage_loading::{
    extract_stage_frontmatter, load_stages_from_work_dir, StageFrontmatter,
};
