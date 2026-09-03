//! Coverage for the `layout: None` branch of `install_all`: every other test
//! in this suite passes `Some(layout)` explicitly, leaving the fallback to
//! `SkillLayout::read` untested.

use std::fs;

use super::paths;
use crate::assets::install::install_all;
use tempfile::TempDir;

#[test]
fn install_all_preserves_the_recorded_layout_when_none_is_given() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    fs::create_dir_all(&paths.claude_dir).unwrap();
    fs::write(
        paths.claude_dir.join("loom-install.toml"),
        "skills = \"core\"\n",
    )
    .unwrap();

    install_all(&paths, None, false).unwrap();

    assert!(paths
        .claude_dir
        .join("loom-skill-catalog/loom-rust/SKILL.md")
        .is_file());
    assert!(!paths.claude_dir.join("skills/loom-rust").exists());
}
