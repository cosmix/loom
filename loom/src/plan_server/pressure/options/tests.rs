use super::*;

use std::fs;
use std::sync::Arc;

use crate::plan_server::authoring::Take;
use crate::plan_server::context::BindInfo;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::planning::runtime::runtime_paths_under;
use crate::planning::watch::OwnWrites;
use tempfile::TempDir;

type SlotField = fn(&mut PressureSlots) -> &mut String;

const SLOT_VOCABULARIES: [(SlotField, &[&str]); 6] = [
    (|s| &mut s.claude_model, CLAUDE_MODELS),
    (|s| &mut s.claude_effort, ALLOWED_REASONING_EFFORTS),
    (|s| &mut s.codex_model, CODEX_MODELS),
    (|s| &mut s.codex_effort, CODEX_EFFORTS),
    (|s| &mut s.address_model, CLAUDE_MODELS),
    (|s| &mut s.address_effort, ALLOWED_REASONING_EFFORTS),
];

fn project(files: &[(&str, &str)]) -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    for (relative, contents) in files {
        let path = temp.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

fn slots() -> PressureSlots {
    PressureSlots {
        rounds: 2,
        claude_model: CLAUDE_MODELS[0].into(),
        claude_effort: ALLOWED_REASONING_EFFORTS[0].into(),
        codex_model: CODEX_MODELS[0].into(),
        codex_effort: CODEX_EFFORTS[0].into(),
        address_model: CLAUDE_MODELS[0].into(),
        address_effort: ALLOWED_REASONING_EFFORTS[0].into(),
    }
}

fn start(plan: &str) -> PressureStart {
    PressureStart {
        plan: plan.into(),
        slots: slots(),
    }
}

fn context(read_only: bool) -> (TempDir, PlanContext, BrowserId) {
    let (temp, project) = project(&[("doc/plans/PLAN-a.md", "# A\n")]);
    let problems = if read_only {
        vec![MissingPrereq {
            what: PrereqKind::Tmux,
            message: "missing tmux".into(),
            fix: "install tmux".into(),
        }]
    } else {
        Vec::new()
    };
    let ctx = PlanContext::new(
        project.root,
        ReadOnly::from_problems(problems),
        BindInfo::new("127.0.0.1:7272".parse().unwrap(), "token"),
        OwnWrites::default(),
    )
    .unwrap();
    ctx.extensions()
        .insert(Arc::new(PressureRunning(AtomicBool::new(true))));
    let browser = BrowserId::parse(&"a".repeat(32)).unwrap();
    (temp, ctx, browser)
}

fn assert_disabled(result: PressureOptions, reason: &str) {
    assert!(!result.available);
    assert_eq!(result.disabled_reason.as_deref(), Some(reason));
    assert!(
        result.running,
        "running remains visible behind earlier refusals"
    );
}

#[test]
#[serial_test::serial]
fn codex_missing_precedes_every_other_reason() {
    let (_temp, ctx, browser) = context(true);
    assert_disabled(
        options_with_prerequisites(&ctx, &browser, false, false),
        disabled_reason::CODEX_MISSING,
    );
}

#[test]
#[serial_test::serial]
fn codex_skill_missing_precedes_read_only_and_running() {
    let (_temp, ctx, browser) = context(true);
    assert_disabled(
        options_with_prerequisites(&ctx, &browser, true, false),
        disabled_reason::CODEX_SKILL_MISSING,
    );
}

#[test]
#[serial_test::serial]
fn read_only_precedes_running() {
    let (_temp, ctx, browser) = context(true);
    assert_disabled(
        options_with_prerequisites(&ctx, &browser, true, true),
        disabled_reason::READ_ONLY,
    );
}

#[test]
#[serial_test::serial]
fn another_holder_precedes_running() {
    let (_temp, ctx, holder) = context(false);
    ctx.require_authoring(&holder, Take::TakeIfFree).unwrap();
    let viewer = BrowserId::parse(&"b".repeat(32)).unwrap();
    assert_disabled(
        options_with_prerequisites(&ctx, &viewer, true, true),
        disabled_reason::NOT_HOLDER,
    );
}

#[test]
#[serial_test::serial]
fn running_disables_the_holder() {
    let (_temp, ctx, browser) = context(false);
    ctx.require_authoring(&browser, Take::TakeIfFree).unwrap();
    assert_disabled(
        options_with_prerequisites(&ctx, &browser, true, true),
        disabled_reason::RUNNING,
    );
}

#[test]
#[serial_test::serial]
fn available_with_free_authoring_or_the_requesters_lease() {
    let (_temp, ctx, browser) = context(false);
    ctx.extensions()
        .get::<PressureRunning>()
        .unwrap()
        .0
        .store(false, Ordering::SeqCst);
    for held in [false, true] {
        if held {
            ctx.require_authoring(&browser, Take::TakeIfFree).unwrap();
        }
        let result = options_with_prerequisites(&ctx, &browser, true, true);
        assert!(result.available);
        assert_eq!(result.disabled_reason, None);
        assert!(!result.running);
        assert_eq!(result.plans, ["doc/plans/PLAN-a.md"]);
        assert_eq!(result.defaults, default_slots(ctx.root()));
        assert_eq!(result.defaults.rounds, 1);
        assert_eq!(result.claude_models, CLAUDE_MODELS);
        assert_eq!(result.claude_efforts, ALLOWED_REASONING_EFFORTS);
        assert_eq!(result.codex_models, CODEX_MODELS);
        assert_eq!(result.codex_efforts, CODEX_EFFORTS);
    }
}

#[test]
fn written_plans_follow_metaplan_then_filename_order_without_duplicates() {
    let (_temp, project) = project(&[
        (
            "doc/plans/feature/index.md",
            "# Plans\n\n## First `PLAN-z.md`\n\n## Again `PLAN-z.md`\n\n\
             ## Follow-ups\n\n## Next `PLAN-y.md`\n\n## Again `PLAN-z.md`\n",
        ),
        ("doc/plans/feature/PLAN-a.md", "# A\n"),
        ("doc/plans/feature/PLAN-a-child.md", "# A child\n"),
        ("doc/plans/feature/PLAN-b.md", "# B\n"),
        ("doc/plans/feature/PLAN-y.md", "# Y\n"),
        ("doc/plans/feature/PLAN-z.md", "# Z\n"),
    ]);
    assert_eq!(
        written_plans(&project),
        [
            "doc/plans/feature/PLAN-z.md",
            "doc/plans/feature/PLAN-y.md",
            "doc/plans/feature/PLAN-a-child.md",
            "doc/plans/feature/PLAN-a.md",
            "doc/plans/feature/PLAN-b.md",
        ]
    );
}

#[test]
#[serial_test::serial]
fn plan_not_written_refuses_locked_outside_archived_and_superseded_paths() {
    let (_temp, project) = project(&[
        ("doc/plans/PLAN-a.md", "# A\n"),
        ("doc/plans/IN_PROGRESS-PLAN-b.md", "# B\n"),
        ("doc/plans/PLAN-b.md", "# Stray B\n"),
        ("doc/plans/DONE-PLAN-c.md", "# C\n"),
        ("doc/plans/RETIRED-PLAN-d.md", "# D\n"),
        ("doc/plans/archive/PLAN-e.md", "# E\n"),
        ("doc/product/index.md", "# Vision\n"),
    ]);
    for path in [
        "doc/plans/IN_PROGRESS-PLAN-b.md",
        "doc/plans/PLAN-b.md",
        "doc/plans/DONE-PLAN-c.md",
        "doc/plans/RETIRED-PLAN-d.md",
        "doc/plans/archive/PLAN-e.md",
        "doc/product/index.md",
        "doc/plans/PLAN-missing.md",
        "doc/plans/../plans/PLAN-a.md",
        "doc/plans/./PLAN-a.md",
        "/doc/plans/PLAN-a.md",
    ] {
        assert_eq!(
            validate_start(&project, &start(path)),
            Err("plan_not_written".into()),
            "{path}"
        );
    }
    assert_eq!(written_plans(&project), ["doc/plans/PLAN-a.md"]);
    assert_eq!(
        validate_start(&project, &start("doc/plans/PLAN-a.md")),
        Ok(())
    );
}

#[test]
#[serial_test::serial]
fn invalid_model_refuses_each_of_the_six_slots() {
    let (_temp, project) = project(&[("doc/plans/PLAN-a.md", "# A\n")]);
    for (field, _) in SLOT_VOCABULARIES {
        for invalid in ["", "unknown", "HIGH"] {
            let mut request = start("doc/plans/PLAN-a.md");
            *field(&mut request.slots) = invalid.into();
            assert_eq!(
                validate_start(&project, &request),
                Err("invalid_model".into())
            );
        }
    }
    let mut request = start("doc/plans/PLAN-a.md");
    request.slots.codex_effort = "max".into();
    assert_eq!(
        validate_start(&project, &request),
        Err("invalid_model".into())
    );
}

#[test]
fn every_declared_model_and_effort_is_accepted_in_its_slot() {
    for (field, values) in SLOT_VOCABULARIES {
        for value in values {
            let mut selected = slots();
            *field(&mut selected) = (*value).into();
            assert_eq!(validate_slots(&selected), Ok(()), "{value}");
        }
    }
}

#[test]
#[serial_test::serial]
fn invalid_rounds_refuses_zero_and_values_above_ten() {
    let (_temp, project) = project(&[("doc/plans/PLAN-a.md", "# A\n")]);
    for rounds in [0, 11, u32::MAX] {
        let mut request = start("doc/plans/PLAN-a.md");
        request.slots.rounds = rounds;
        assert_eq!(
            validate_start(&project, &request),
            Err("invalid_rounds".into())
        );
    }
    for rounds in [1, 10] {
        assert_eq!(validate_slots(&PressureSlots { rounds, ..slots() }), Ok(()));
    }
}

#[test]
fn config_blocked_refuses_persisted_blocks_and_clearing_allows_start() {
    let (temp, project) = project(&[("doc/plans/PLAN-a.md", "# A\n")]);
    let runtime = runtime_paths_under(&temp.path().join("user"), &project.root).unwrap();
    assert_eq!(validate_config(&runtime), Ok(()));
    StartBlocks::block(&runtime, "hooks changed", &[".claude/settings.json".into()]).unwrap();
    assert_eq!(validate_config(&runtime), Err("config_blocked".into()));
    StartBlocks::clear(&runtime).unwrap();
    assert_eq!(validate_config(&runtime), Ok(()));
}

#[test]
fn config_blocked_fails_closed_on_malformed_state() {
    let (temp, project) = project(&[("doc/plans/PLAN-a.md", "# A\n")]);
    let runtime = runtime_paths_under(&temp.path().join("user"), &project.root).unwrap();
    StartBlocks::block(&runtime, "hooks changed", &[]).unwrap();
    fs::write(runtime.dir.join("config-block.json"), "not json").unwrap();
    assert_eq!(validate_config(&runtime), Err("config_blocked".into()));
}
