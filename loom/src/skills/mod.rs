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
// `core_skill_names`, `is_core_skill`, `load_from_roots`, `CATALOG_DIR_NAME`,
// and `SkillLayout` are reached in-crate by their own module paths (e.g.
// `super::index_catalog::is_core_skill` from `install_layout.rs`); adding
// them here would be API surface with no caller (Engineering Discipline B).
pub use index_catalog::{catalog_dir_for, load_with_catalog, skill_invocation};
pub use install_layout::apply_install_layout;
pub use types::{SkillMatch, SkillMetadata};
