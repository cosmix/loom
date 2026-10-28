use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::{brief, AuthoringStart, CLOSING_MESSAGE};
use crate::assets::{Asset, CLAUDE_COMMANDS, PLAN_PLUGIN};
use crate::planning::checklist::Offer;
use crate::planning::project::Project;
use crate::planning::slug::PlanState;

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
    brief::common_brief(&AuthoringStart::NoStep, &project).unwrap()
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
    assert_eq!(count, 9, "all nine planning skills must be checked");
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
fn starting_point_without_documents_has_no_skill_and_reports_absence() {
    let text = common_brief();
    let (_, start) = text.split_once("## Starting point\n\n").unwrap();
    assert_eq!(
        start,
        concat!(
            "Step: none chosen.\nTarget: Ask me what I want to work on.\n\n",
            "Project state at launch (it may change; re-read the files before relying on it):\n",
            "- Vision: absent.\n- Metaplans: none.\n",
            "- Plans: 0 written (0 unlisted), 0 running, 0 done, 0 retired.\n",
            "- Locked plan files: none.\n",
        )
    );
    assert!(!start.contains("Skill:"));
}

#[test]
fn starting_point_reports_loaded_vision_metaplan_and_running_plan() {
    let (_temp, project) = fixture(&[
        (
            "doc/product/index.md",
            "# Vision\n\n## Specification sections\n\n\
            | # | Section |\n| --- | --- |\n| 01 | Alpha |\n| 07 | Beta |\n",
        ),
        ("doc/product/section_01.md", "# Alpha\n"),
        ("doc/product/section_07.md", "# Beta\n"),
        (
            "doc/plans/index.md",
            "# Metaplan\n\n## 1. Delivery (`PLAN-foo.md`)\n",
        ),
        (
            "doc/plans/IN_PROGRESS-PLAN-foo.md",
            "# Plan\n\nImplements: section_01\n",
        ),
    ]);
    let text = brief::common_brief(&AuthoringStart::NoStep, &project).unwrap();
    for line in [
        "- Vision: present, 2 sections listed, 2 written (01, 07).",
        "- Metaplans: doc/plans/index.md.",
        "- Plans: 0 written (0 unlisted), 1 running, 0 done, 0 retired.",
        "- Locked plan files: doc/plans/IN_PROGRESS-PLAN-foo.md.",
    ] {
        assert!(
            text.lines().any(|actual| actual == line),
            "missing {line:?}"
        );
    }
}

#[test]
fn starting_point_uses_checklist_counts_and_sorts_only_locked_plan_files() {
    let (_temp, mut project) = fixture(&[]);
    let state = &mut project.checklist.state;
    state.plans_by_state = [
        (PlanState::Written, 3),
        (PlanState::Running, 2),
        (PlanState::Done, 4),
        (PlanState::Retired, 1),
    ]
    .into();
    state.plans_unlisted = 1;
    state.metaplans = ["doc/plans/metaplan-02.md", "doc/plans/index.md"]
        .map(Into::into)
        .into();
    state.locked_files = [
        "doc/plans/RETIRED-PLAN-z.md",
        "doc/plans/briefs/foo/stage.md",
        "doc/plans/IN_PROGRESS-PLAN-m.md",
        "doc/plans/REVIEW-PLAN-foo.md",
        "doc/plans/DONE-PLAN-a.md",
    ]
    .map(Into::into)
    .into();
    let text = brief::common_brief(&AuthoringStart::NoStep, &project).unwrap();
    assert!(text.contains("- Plans: 3 written (1 unlisted), 2 running, 4 done, 1 retired.\n"));
    assert!(text.contains("- Metaplans: doc/plans/index.md, doc/plans/metaplan-02.md.\n"));
    assert_eq!(
        text.lines().last().unwrap(),
        concat!(
            "- Locked plan files: doc/plans/DONE-PLAN-a.md, ",
            "doc/plans/IN_PROGRESS-PLAN-m.md, doc/plans/RETIRED-PLAN-z.md.",
        )
    );
}

#[test]
fn starting_point_names_offer_refinement_knowledge_and_message_skills() {
    let (_temp, project) = fixture(&[]);
    for (start, expected) in [
        (
            AuthoringStart::Offer(Offer::StartVision),
            concat!(
                "Step: 1, Vision.\nTarget: Write doc/product/index.md, the product vision.\n",
                "Skill: loom-plan:vision.\n"
            ),
        ),
        (
            AuthoringStart::Offer(Offer::RefineSpec),
            concat!(
                "Step: done, Review what was built.\nTarget: Review the done plans and their ",
                "run reviews against the product spec.\nSkill: loom-plan:retro.\n"
            ),
        ),
        (
            AuthoringStart::Refine {
                target: "doc/product/section_07.md".into(),
                passage: true,
            },
            concat!(
                "Step: any, Refine.\nTarget: Refine doc/product/section_07.md. ",
                "The message that follows attaches the passage.\nSkill: loom-plan:refine.\n"
            ),
        ),
        (
            AuthoringStart::Knowledge {
                open_file: Some("doc/loom/knowledge/index.md".into()),
            },
            concat!(
                "Step: any, Knowledge.\nTarget: Change the knowledge base under ",
                "doc/loom/knowledge/, starting from doc/loom/knowledge/index.md.\n",
                "Skill: loom-plan:knowledge.\n"
            ),
        ),
        (
            AuthoringStart::Message { paths: vec![] },
            "Step: any, Refine.\nSkill: loom-plan:refine.\n",
        ),
    ] {
        let text = brief::common_brief(&start, &project).unwrap();
        let (_, block) = text.split_once("## Starting point\n\n").unwrap();
        assert_eq!(block.split_once("\nProject state").unwrap().0, expected);
    }
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
            vec!["doc/product/index.md", "Specification sections"],
        ),
        ("section", vec!["doc/product/section_", "## Open questions"]),
        ("metaplan", vec!["doc/plans/index.md", "metaplan-"]),
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
