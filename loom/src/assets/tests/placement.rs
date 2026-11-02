//! Regression coverage for the symlink-escape guard in `write_skill`: a
//! subdirectory below the skill root that is itself a symlink must be
//! unlinked and replaced, not followed.

use std::fs;
use std::os::unix::fs::symlink;

use crate::assets::install::placement::place_skill;
use crate::assets::Asset;
use tempfile::TempDir;

#[test]
fn symlinked_subdirectory_below_the_skill_root_is_not_followed() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join("skills");
    let catalog_dir = temp.path().join("catalog");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let skill_root = skills_dir.join("loom-test-skill");
    fs::create_dir_all(&skill_root).unwrap();
    symlink(&outside, skill_root.join("nested")).unwrap();

    let assets: &[Asset] = &[("loom-test-skill/nested/file.md", "body")];
    place_skill("loom-test-skill", assets, &skills_dir, &catalog_dir, true).unwrap();

    assert_eq!(
        fs::read_to_string(skill_root.join("nested/file.md")).unwrap(),
        "body"
    );
    assert!(
        fs::read_dir(&outside).unwrap().next().is_none(),
        "the symlink target must stay untouched"
    );
}

#[test]
fn shipped_leaf_symlinks_are_replaced_without_touching_targets() {
    for relative in ["SKILL.md", "nested/file.md"] {
        for dangling in [false, true] {
            let temp = TempDir::new().unwrap();
            let skills = temp.path().join("skills");
            let catalog = temp.path().join("catalog");
            let leaf = skills.join("loom-test-skill").join(relative);
            let outside = temp.path().join("outside");
            fs::create_dir_all(leaf.parent().unwrap()).unwrap();
            if !dangling {
                fs::write(&outside, "untouched").unwrap();
            }
            symlink(&outside, &leaf).unwrap();
            let assets: &[Asset] = &[
                ("loom-test-skill/SKILL.md", "skill"),
                ("loom-test-skill/nested/file.md", "nested"),
            ];
            place_skill("loom-test-skill", assets, &skills, &catalog, true).unwrap();
            assert!(fs::symlink_metadata(&leaf).unwrap().file_type().is_file());
            assert_eq!(
                fs::read_to_string(&leaf).unwrap(),
                if relative == "SKILL.md" {
                    "skill"
                } else {
                    "nested"
                }
            );
            if dangling {
                assert!(!outside.exists());
            } else {
                assert_eq!(fs::read_to_string(outside).unwrap(), "untouched");
            }
        }
    }
}

#[test]
fn directory_at_a_shipped_file_preserves_both_roots_on_failure() {
    let temp = TempDir::new().unwrap();
    let skills = temp.path().join("skills");
    let catalog = temp.path().join("catalog");
    let leaf = skills.join("loom-test-skill/SKILL.md");
    fs::create_dir_all(&leaf).unwrap();
    let other = catalog.join("loom-test-skill/SKILL.md");
    fs::create_dir_all(other.parent().unwrap()).unwrap();
    fs::write(&other, "previous").unwrap();
    let assets: &[Asset] = &[("loom-test-skill/SKILL.md", "new")];
    assert!(place_skill("loom-test-skill", assets, &skills, &catalog, true).is_err());
    assert!(leaf.is_dir());
    assert_eq!(fs::read_to_string(other).unwrap(), "previous");
}

#[test]
fn real_installers_replace_skill_leaf_links() {
    let temp = TempDir::new().unwrap();
    let paths = super::paths(&temp);
    let outside = temp.path().join("outside");
    fs::write(&outside, "untouched").unwrap();
    for root in [&paths.claude_dir, &paths.codex_dir] {
        let leaf = root.join("skills/loom-plan-writer/SKILL.md");
        fs::create_dir_all(leaf.parent().unwrap()).unwrap();
        symlink(&outside, leaf).unwrap();
    }
    super::install(&temp, crate::skills::SkillLayout::All);
    assert_eq!(fs::read_to_string(outside).unwrap(), "untouched");
    for root in [&paths.claude_dir, &paths.codex_dir] {
        let leaf = root.join("skills/loom-plan-writer/SKILL.md");
        assert!(fs::symlink_metadata(&leaf).unwrap().file_type().is_file());
        assert_eq!(
            fs::read_to_string(leaf).unwrap(),
            super::asset_content(crate::assets::SKILLS, "loom-plan-writer/SKILL.md")
        );
    }
}

/// A synthetic asset key with a `..` component lexically "starts with" the
/// skill root (`root/..` is a real directory), so `strip_prefix` alone would
/// accept it and send the write outside the tree loom was asked to write.
/// Only `assert_clean_keys` (a compile-time test over the real embedded
/// tables) stops such a key existing today; this pins the runtime guard that
/// would still catch it if that compile-time check were ever bypassed.
#[test]
fn skill_asset_key_containing_dot_dot_is_rejected() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join("skills");
    let catalog_dir = temp.path().join("catalog");
    fs::create_dir_all(&skills_dir).unwrap();

    let assets: &[Asset] = &[("loom-test-skill/../escape/file.md", "body")];
    let error =
        place_skill("loom-test-skill", assets, &skills_dir, &catalog_dir, true).unwrap_err();

    assert!(format!("{error:#}").contains("escapes"), "{error:#}");
    assert!(!temp.path().join("escape").exists());
    assert!(!skills_dir
        .join("loom-test-skill")
        .join("../escape")
        .exists());
}
