//! Contracts for `loom::planning::guidance`: the per-session brief, opening message and
//! `loom-plan` plugin render, the authoring model keys, offer-id lookup, and the plan-writer
//! template's `Implements:` line.

#[path = "planning_guidance_contracts/support.rs"]
mod support;

use loom::planning::checklist::Offer;
use loom::planning::guidance::{
    opening_message, step_class, AuthoringStart, PressureCommand, SessionGuidanceKind, StepClass,
};
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use support::{files, frontmatter, listing, put, repo_file, Fixture};

const SKILLS: [&str; 10] = [
    "brainstorming",
    "kickoff",
    "knowledge",
    "metaplan",
    "plan",
    "refine",
    "retro",
    "section",
    "summaries",
    "vision",
];

fn write_plan() -> AuthoringStart {
    AuthoringStart::Offer(Offer::WritePlan {
        feature: "f".into(),
        metaplan: PathBuf::from("doc/plans/f/index.md"),
        slug: "delivery-truth".to_string(),
    })
}

fn write_section_3() -> AuthoringStart {
    AuthoringStart::Offer(Offer::WriteSection {
        feature: "f".into(),
        number: 3,
    })
}

#[test]
#[serial]
fn authoring_plugin_carries_skills_and_agents_only() {
    let fx = Fixture::new();
    let models = fx.models("");
    let kind = SessionGuidanceKind::Authoring(AuthoringStart::NoStep);
    fx.render(&kind, &fx.session, &models).unwrap();
    let plugin = fx.session.plugin();
    let mut expected: Vec<String> = SKILLS
        .iter()
        .map(|name| format!("skills/{name}/SKILL.md"))
        .collect();
    expected.push(".claude-plugin/plugin.json".to_string());
    expected.push("agents/summary.md".to_string());
    expected.push("agents/overview.md".to_string());
    expected.sort();
    assert_eq!(files(&plugin), expected, "plugin files");
    assert!(
        !listing(&plugin)
            .iter()
            .any(|rel| rel.starts_with("commands")),
        "an authoring plugin carries no commands/ entry"
    );
}

#[test]
#[serial]
fn summary_plugin_carries_only_summaries_skill() {
    let fx = Fixture::new();
    let models = fx.models("");
    let kind = SessionGuidanceKind::Summary {
        ids: vec!["overview/f/product".to_string()],
    };
    fx.render(&kind, &fx.session, &models).unwrap();
    let plugin = fx.session.plugin();
    assert_eq!(
        listing(&plugin.join("skills")),
        vec!["summaries/".to_string(), "summaries/SKILL.md".to_string()],
        "a summary session carries only the summaries skill"
    );
    assert!(plugin.join("agents/summary.md").is_file(), "summary agent");
    assert!(
        plugin.join("agents/overview.md").is_file(),
        "overview agent"
    );
}

#[test]
#[serial]
fn pressure_plugin_commands_match_repository_copies() {
    let fx = Fixture::new();
    let models = fx.models("");
    let kind = SessionGuidanceKind::Pressure {
        command: PressureCommand::Pressure,
        plan_rel: "doc/plans/PLAN-delivery-truth.md".to_string(),
        brief: "B".to_string(),
        model: "opus".to_string(),
        effort: "high".to_string(),
    };
    let guidance = fx.render(&kind, &fx.session, &models).unwrap();
    let plugin = fx.session.plugin();
    for name in ["pressure.md", "address.md"] {
        let rendered = fs::read(plugin.join("commands").join(name)).unwrap();
        let source = fs::read(repo_file("../commands").join(name)).unwrap();
        assert!(
            rendered == source,
            "commands/{name} differs from the repository copy"
        );
    }
    assert!(
        !plugin.join("skills").exists(),
        "a pressure plugin has no skills/"
    );
    assert!(
        !plugin.join("agents").exists(),
        "a pressure plugin has no agents/"
    );
    assert_eq!(fs::read_to_string(fx.session.brief()).unwrap(), "B");
    assert_eq!(
        guidance.opening_message,
        "/loom-plan:pressure doc/plans/PLAN-delivery-truth.md"
    );
}

#[test]
#[serial]
fn agent_frontmatter_follows_non_default_config() {
    let fx = Fixture::new();
    let models = fx.models(
        "[plan]\ndocument_summary_model = \"sonnet\"\ndocument_summary_effort = \"low\"\n\
         overview_model = \"opus\"\noverview_effort = \"max\"\n",
    );
    let kind = SessionGuidanceKind::Summary {
        ids: vec!["overview/f/product".to_string()],
    };
    let guidance = fx.render(&kind, &fx.session, &models).unwrap();
    let agents = fx.session.plugin().join("agents");
    let summary = frontmatter(&fs::read_to_string(agents.join("summary.md")).unwrap());
    let has = |lines: &[String], want: &str| lines.iter().any(|line| line == want);
    assert!(has(&summary, "model: sonnet"), "summary agent: {summary:?}");
    assert!(has(&summary, "effort: low"), "summary agent: {summary:?}");
    assert!(!has(&summary, "model: haiku"), "summary agent: {summary:?}");
    let overview = frontmatter(&fs::read_to_string(agents.join("overview.md")).unwrap());
    assert!(
        has(&overview, "model: opus"),
        "overview agent: {overview:?}"
    );
    assert!(
        has(&overview, "effort: max"),
        "overview agent: {overview:?}"
    );
    assert_eq!(guidance.model, "opus");
    assert_eq!(guidance.effort, "max");
}

#[test]
#[serial]
fn plan_step_uses_plan_authoring_keys() {
    let fx = Fixture::new();
    let models = fx.models(
        "[plan]\nplan_authoring_model = \"sonnet\"\nplan_authoring_effort = \"low\"\n\
         spec_authoring_model = \"haiku\"\nspec_authoring_effort = \"max\"\n",
    );
    assert!(matches!(step_class(&write_plan()), StepClass::Plan));
    assert!(matches!(step_class(&write_section_3()), StepClass::Spec));
    let plan = SessionGuidanceKind::Authoring(write_plan());
    let guidance = fx.render(&plan, &fx.session, &models).unwrap();
    assert_eq!(
        (guidance.model.as_str(), guidance.effort.as_str()),
        ("sonnet", "low")
    );
    let section = SessionGuidanceKind::Authoring(write_section_3());
    let second = fx.new_session("s2");
    let guidance = fx.render(&section, &second, &models).unwrap();
    assert_eq!(
        (guidance.model.as_str(), guidance.effort.as_str()),
        ("haiku", "max")
    );
}

#[test]
#[serial]
fn brainstorming_copy_keeps_mit_notice() {
    let fx = Fixture::new();
    let models = fx.models("");
    let kind = SessionGuidanceKind::Authoring(AuthoringStart::NoStep);
    fx.render(&kind, &fx.session, &models).unwrap();
    let license_path = repo_file(
        "../doc/plans/planner-ui/briefs/feat-planner-08-guidance/upstream/superpowers-LICENSE",
    );
    let license = fs::read_to_string(license_path).unwrap();
    let license = license.trim_end_matches('\n');
    assert!(license.contains("Copyright (c) 2025 Jesse Vincent"));
    assert!(license.contains("Permission is hereby granted"));
    let skill = fx.session.plugin().join("skills/brainstorming/SKILL.md");
    let skill = fs::read_to_string(skill).unwrap();
    assert!(
        skill.contains(license),
        "the brainstorming skill lacks the MIT notice as one block:\n{skill}"
    );
}

#[test]
#[serial]
fn starting_point_block_counts_the_fixture() {
    let fx = Fixture::new();
    let models = fx.models("");
    let kind = SessionGuidanceKind::Authoring(write_section_3());
    let guidance = fx.render(&kind, &fx.session, &models).unwrap();
    assert_eq!(guidance.brief_path, fx.session.brief());
    let brief = fs::read_to_string(fx.session.brief()).unwrap();
    for want in [
        "## Starting point",
        "Skill: loom-plan:section.",
        "- Vision: present, 3 sections listed, 2 written (01, 02).",
        "- Locked plan files: doc/plans/f/IN_PROGRESS-PLAN-b.md.",
    ] {
        assert!(
            brief.lines().any(|line| line == want),
            "brief.md lacks the line {want:?}:\n{brief}"
        );
    }
}

#[test]
fn write_plan_opening_names_entry_heading() {
    let fx = Fixture::new();
    let message = opening_message(&write_plan(), &fx.project, Some("f")).unwrap();
    assert_eq!(
        message.lines().take(3).collect::<Vec<_>>().join("\n"),
        "loom plan: step 4, Plans.\n\
         Write doc/plans/f/PLAN-delivery-truth.md for the entry \"1. Delivery truth\" in \
         doc/plans/f/index.md. Write its briefs under doc/plans/f/briefs/delivery-truth/.\n\
         Load the skill loom-plan:plan, then begin by telling me what you read and what you \
         plan to ask."
    );
}

#[test]
fn offer_id_lookup_rejects_unknown_ids() {
    let fx = Fixture::new();
    let primary = fx
        .project
        .feature("f")
        .unwrap()
        .checklist
        .primary
        .clone()
        .expect("a primary offer");
    let id = primary.id();
    let found = AuthoringStart::from_offer_id(&id, &fx.project, Some("f"));
    assert!(
        matches!(found, Ok(AuthoringStart::Offer(ref offer)) if *offer == primary),
        "primary offer {id} not found as itself"
    );
    let refine =
        AuthoringStart::from_offer_id("refine:product/f/section_01.md", &fx.project, Some("f"));
    assert!(refine.is_ok(), "refine of an existing section");
    for id in [
        "write-section:99",
        "",
        "../x",
        "refine:../x",
        "refine:product/missing.md",
        "start-run",
    ] {
        let found = AuthoringStart::from_offer_id(id, &fx.project, Some("f"));
        assert!(found.is_err(), "{id:?} gave a start");
    }
}

#[test]
#[serial]
fn render_refuses_symlinked_plugin_dir() {
    let fx = Fixture::new();
    let models = fx.models("");
    let target = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(target.path(), fx.session.plugin()).unwrap();
    let kind = SessionGuidanceKind::Authoring(AuthoringStart::NoStep);
    let result = fx.render(&kind, &fx.session, &models);
    assert!(result.is_err(), "rendered through a symlinked plugin/");
    assert_eq!(
        listing(target.path()),
        Vec::<String>::new(),
        "link target written"
    );
    assert!(!fx.session.brief().exists(), "brief.md written");
}

#[test]
fn template_skeleton_carries_implements_and_verifies() {
    let path = repo_file("../skills/loom-plan-writer/references/plan-template.md");
    let template = fs::read_to_string(path).unwrap();
    let block: Vec<&str> = template
        .lines()
        .skip_while(|line| !line.starts_with("````markdown"))
        .skip(1)
        .take_while(|line| *line != "````")
        .collect();
    assert!(!block.is_empty(), "no four-backtick markdown block");
    let mut non_empty = block.iter().filter(|line| !line.trim().is_empty());
    let second = non_empty.nth(1).expect("a second non-empty line");
    assert!(second.starts_with("Implements:"), "second line: {second:?}");
    let temp = tempfile::tempdir().unwrap();
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(temp.path())
        .status()
        .unwrap();
    assert!(init.success(), "git init");
    put(
        temp.path(),
        "doc/plans/PLAN-tpl.md",
        block.join("\n") + "\n",
    );
    let mut verify = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            verify.env_remove(key);
        }
    }
    let out = verify
        .args(["plan", "verify", "doc/plans/PLAN-tpl.md"])
        .current_dir(temp.path())
        .env("LOOM_HOME", temp.path().join("home"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "loom plan verify failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[path = "planning_guidance_contracts/features.rs"]
mod features;
