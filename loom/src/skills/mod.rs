//! Skill routing module for automated skill recommendations in signals
//!
//! This module provides functionality to:
//! - Load skill metadata from SKILL.md files in ~/.claude/skills/ and its
//!   sibling `~/.claude/loom-skill-catalog/` (skills Claude Code does not
//!   index directly — see `index_catalog`)
//! - Build an inverted index of trigger keywords
//! - Match stage descriptions against triggers to recommend relevant skills
//!
//! # Example
//!
//! ```ignore
//! use loom::skills::load_with_catalog;
//! use std::path::Path;
//!
//! let index = load_with_catalog(Path::new("~/.claude/skills/"))?;
//! let matches = index.match_skills("implement OAuth login flow", 5);
//!
//! for skill in matches {
//!     println!("Recommended: {} (score: {})", skill.name, skill.score);
//! }
//! ```

mod index;
mod index_catalog;
mod install_layout;
mod matcher;
mod types;

pub use index::SkillIndex;
// Only names with a consumer outside `src/skills/` are re-exported here.
// `crate::assets::install` consumes the placement layout and catalog names;
// other internal callers continue to use their local module paths.
pub use index_catalog::{
    catalog_dir_for, is_core_skill, load_with_catalog, skill_invocation, CATALOG_DIR_NAME,
};
pub use install_layout::SkillLayout;
pub use types::{SkillMatch, SkillMetadata};
