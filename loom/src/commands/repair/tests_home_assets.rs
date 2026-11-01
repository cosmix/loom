use super::*;
use crate::assets::{CLAUDE_AGENTS, SKILLS};
use std::fs;

#[test]
fn shipped_legacy_assets_are_backed_up_and_custom_assets_survive() {
    let root = tempfile::tempdir().unwrap();
    let skills = root.path().join("skills");
    fs::create_dir_all(skills.join("loom-rust")).unwrap();
    fs::create_dir_all(skills.join("rust")).unwrap();
    fs::create_dir_all(skills.join("loom-python")).unwrap();
    fs::create_dir_all(skills.join("python")).unwrap();
    fs::write(skills.join("python/SKILL.md"), "operator skill").unwrap();

    for (path, body) in SKILLS {
        if let Some(relative) = path.strip_prefix("loom-rust/") {
            let dest = skills.join("rust").join(relative);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::write(dest, body.replacen("name: loom-rust\n", "name: rust\n", 1)).unwrap();
        }
    }
    let skills_issues = home_assets::check_old_skills_at(&skills);
    assert_eq!(skills_issues.len(), 2);
    let skill = skills_issues
        .iter()
        .find(|issue| matches!(&issue.action, RepairAction::OldSkill(_)))
        .unwrap();
    let custom = skills_issues
        .iter()
        .find(|issue| issue.action == RepairAction::Manual)
        .unwrap();
    assert!(!fix_issue(root.path(), custom).unwrap());
    assert!(fix_issue(root.path(), skill).unwrap());
    assert!(!skills.join("rust").exists());
    assert!(skills.join("python/SKILL.md").exists());
    let skill_backups: Vec<_> = fs::read_dir(&skills)
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("rust.loom-backup-")
        })
        .collect();
    assert_eq!(skill_backups.len(), 1);
    assert!(skill_backups[0].path().join("SKILL.md").exists());
}

#[test]
fn shipped_legacy_agent_is_backed_up() {
    let root = tempfile::tempdir().unwrap();
    let agents = root.path().join("agents");
    fs::create_dir_all(&agents).unwrap();
    let body = CLAUDE_AGENTS
        .iter()
        .find(|(path, _)| *path == "loom-software-engineer.md")
        .unwrap()
        .1;
    fs::write(
        agents.join("software-engineer.md"),
        body.replacen(
            "name: loom-software-engineer\n",
            "name: software-engineer\n",
            1,
        ),
    )
    .unwrap();
    fs::write(agents.join("loom-software-engineer.md"), body).unwrap();
    let agent_issue = home_assets::check_old_agents_at(&agents).pop().unwrap();
    assert!(matches!(&agent_issue.action, RepairAction::OldAgent(_)));
    assert!(fix_issue(root.path(), &agent_issue).unwrap());
    let backups: Vec<_> = fs::read_dir(&agents)
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("software-engineer.md.loom-backup-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::read_to_string(backups[0].path()).unwrap(),
        body.replacen(
            "name: loom-software-engineer\n",
            "name: software-engineer\n",
            1
        )
    );
}

#[test]
fn ownership_is_rechecked_before_migration() {
    let root = tempfile::tempdir().unwrap();
    let agents = root.path().join("agents");
    fs::create_dir_all(&agents).unwrap();
    let body = CLAUDE_AGENTS
        .iter()
        .find(|(path, _)| *path == "loom-software-engineer.md")
        .unwrap()
        .1;
    fs::write(agents.join("software-engineer.md"), body).unwrap();
    fs::write(agents.join("loom-software-engineer.md"), body).unwrap();
    let issue = home_assets::check_old_agents_at(&agents).pop().unwrap();
    fs::write(agents.join("software-engineer.md"), "operator edit").unwrap();
    assert!(fix_issue(root.path(), &issue).is_err());
    assert_eq!(
        fs::read_to_string(agents.join("software-engineer.md")).unwrap(),
        "operator edit"
    );
}

#[test]
fn skill_permission_migration_matches_complete_names_only() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.json");
    let original = serde_json::json!({
        "permissions": {
            "allow": ["Skill(rust)", "Skill(rust:*)", "Skill(rustic)", "Skill(rust's)", "Skill(loom-rust)"],
            "deny": ["Skill(python)", "Skill(python-extra)"],
            "ask": ["Skill(testing)"]
        },
        "note": "Skill(rust)"
    });
    fs::write(&path, original.to_string()).unwrap();
    assert_eq!(home_assets::check_old_skill_refs_at(&path).len(), 1);
    home_assets::fix_settings_skill_refs_at(&path).unwrap();
    let updated: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        updated["permissions"]["allow"],
        serde_json::json!([
            "Skill(loom-rust)",
            "Skill(loom-rust:*)",
            "Skill(rustic)",
            "Skill(rust's)",
            "Skill(loom-rust)"
        ])
    );
    assert_eq!(
        updated["permissions"]["deny"],
        serde_json::json!(["Skill(loom-python)", "Skill(python-extra)"])
    );
    assert_eq!(
        updated["permissions"]["ask"],
        serde_json::json!(["Skill(loom-testing)"])
    );
    assert_eq!(updated["note"], original["note"]);
    assert!(home_assets::check_old_skill_refs_at(&path).is_empty());
}
