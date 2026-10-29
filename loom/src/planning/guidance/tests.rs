use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::{brief, AuthoringStart, CLOSING_MESSAGE};
use crate::assets::{Asset, CLAUDE_COMMANDS, PLAN_PLUGIN};
use crate::planning::project::Project;

fn asset(assets: &[Asset], key: &str) -> &'static str {
    assets
        .iter()
        .find_map(|&(path, text)| (path == key).then_some(text))
        .unwrap_or_else(|| panic!("missing embedded asset: {key}"))
}

fn product_lines(file: &str, first: usize, last: usize) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../doc/product")
        .join(file);
    let text = fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = text
        .lines()
        .skip(first - 1)
        .take(last - first + 1)
        .collect();
    assert_eq!(
        lines.len(),
        last - first + 1,
        "{} is too short",
        path.display()
    );
    lines.join("\n")
}

fn fixture(files: &[(&str, &str)]) -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("doc")).unwrap();
    for (path, text) in files {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

fn common_brief() -> String {
    let (_temp, project) = fixture(&[]);
    brief::common_brief(&AuthoringStart::NoStep, &project, None).unwrap()
}

#[test]
fn brainstorming_omits_upstream_paths_handoffs_and_commits() {
    let text = asset(PLAN_PLUGIN, "skills/brainstorming/SKILL.md");
    for forbidden in ["docs/superpowers", "writing-plans", "git commit"] {
        assert!(
            !text.contains(forbidden),
            "brainstorming contains {forbidden:?}"
        );
    }
}

#[test]
fn every_skill_has_matching_name_and_description_in_frontmatter() {
    let mut count = 0;
    for &(path, text) in PLAN_PLUGIN {
        let Some(name) = path
            .strip_prefix("skills/")
            .and_then(|path| path.strip_suffix("/SKILL.md"))
        else {
            continue;
        };
        count += 1;
        assert!(text.starts_with(&format!("---\nname: {name}\n")), "{path}");
        let frontmatter = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .expect("closing frontmatter delimiter")
            .0;
        assert!(
            frontmatter
                .lines()
                .any(|line| line.starts_with("description: ")),
            "{path} is missing a frontmatter description"
        );
    }
    assert_eq!(count, 10, "all ten planning skills must be checked");
}

#[test]
fn knowledge_skill_preserves_every_distill_routing_line() {
    let knowledge = asset(PLAN_PLUGIN, "skills/knowledge/SKILL.md");
    let distill = asset(CLAUDE_COMMANDS, "distill.md");
    let lines: Vec<_> = distill.lines().skip(13).take(6).collect();
    assert_eq!(lines.len(), 6);
    for line in lines {
        assert!(
            line.starts_with("- "),
            "distill.md lines 14-19 are no longer routing bullets: {line:?}"
        );
        assert!(
            knowledge.contains(line),
            "knowledge skill is missing {line:?}"
        );
    }
}

#[test]
fn common_brief_quotes_product_writing_conventions_verbatim() {
    let conventions = product_lines("section_03.md", 243, 251);
    assert!(
        common_brief().contains(&conventions),
        "section 03 conventions drifted"
    );
}

#[test]
fn closing_message_matches_product_spec_with_continuation_indent() {
    assert_eq!(CLOSING_MESSAGE, product_lines("section_09.md", 320, 325));
}

#[test]
fn plugin_manifest_is_json_named_loom_plan() {
    let manifest: serde_json::Value =
        serde_json::from_str(asset(PLAN_PLUGIN, ".claude-plugin/plugin.json")).unwrap();
    assert_eq!(manifest["name"], "loom-plan");
}

#[test]
fn installer_does_not_install_the_session_plugin() {
    assert!(!include_str!("../../assets/install.rs").contains("PLAN_PLUGIN"));
}

#[test]
fn common_brief_orders_all_six_parts_before_the_starting_point() {
    let text = common_brief();
    let offsets: Vec<_> = [
        "## What this session is\n",
        "## The documents are the truth\n",
        "## What the writing sessions keep to\n",
        "## What this session may write\n",
        "## The turn-end duty\n",
        "## The closing message\n",
        "## Starting point\n",
    ]
    .iter()
    .map(|marker| {
        text.find(marker)
            .unwrap_or_else(|| panic!("common brief is missing {marker:?}"))
    })
    .collect();
    assert!(
        offsets.windows(2).all(|pair| pair[0] < pair[1]),
        "{offsets:?}"
    );
}

#[test]
fn common_brief_carries_all_write_limits_and_the_writing_plans_prohibition() {
    let text = common_brief();
    for lead in [
        "Documents only",
        "Locked plans",
        "Run reviews",
        "Knowledge",
        "Summaries",
        "No git and no execution",
    ] {
        assert!(
            text.contains(&format!("- **{lead}.**")),
            "missing write limit: {lead}"
        );
    }
    assert!(text.contains("superpowers:writing-plans"));
}

#[test]
fn summary_brief_rebuilds_only_summaries_without_questions() {
    let text = brief::summary_brief();
    for phrase in [
        "Rebuild summaries and nothing else.",
        "Never ask the user anything",
        "loom-plan:summaries",
        "- **Knowledge.**",
        "- **Summaries.**",
        "- **No git and no execution.**",
    ] {
        assert!(text.contains(phrase), "summary brief is missing {phrase:?}");
    }
}

#[test]
fn step_skills_carry_their_target_and_document_conventions() {
    for (skill, phrases) in [
        (
            "vision",
            vec!["doc/product/<feature>/index.md", "Specification sections"],
        ),
        (
            "section",
            vec!["doc/product/<feature>/section_<nn>.md", "## Open questions"],
        ),
        (
            "metaplan",
            vec!["doc/plans/<feature>/index.md", "metaplan-"],
        ),
        ("plan", vec!["Implements:", "loom-plan-writer"]),
        ("refine", vec!["## Open questions"]),
        ("retro", vec!["REVIEW-"]),
        (
            "knowledge",
            vec!["Do not run `loom memory pending` or `loom memory resolve`."],
        ),
    ] {
        let text = asset(PLAN_PLUGIN, &format!("skills/{skill}/SKILL.md"));
        for phrase in phrases {
            assert!(text.contains(phrase), "{skill} is missing {phrase:?}");
        }
    }
}

#[test]
fn every_step_skill_checks_conventions_and_summary_status() {
    for skill in ["vision", "section", "metaplan", "plan", "refine"] {
        let text = asset(PLAN_PLUGIN, &format!("skills/{skill}/SKILL.md"));
        for command in ["loom plan check", "loom plan summary status --json"] {
            assert!(text.contains(command), "{skill} is missing {command:?}");
        }
    }
}

#[test]
fn both_summary_agents_publish_through_seal() {
    for key in ["agents/summary.md", "agents/overview.md"] {
        assert!(
            asset(PLAN_PLUGIN, key).contains("loom plan summary seal"),
            "{key}"
        );
    }
}

mod blocks;
mod drafting;
mod starting;
