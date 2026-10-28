use super::*;
use std::fs;
use tempfile::TempDir;

const VISION: &str = "# Vision\n\n## Specification sections\n\n\
    | # | Section | Status |\n| --- | --- | --- |\n\
    | 01 | Alpha | Written |\n  | 07  |  Café  |  Pending  |  \n";

fn fixture() -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("doc/product/index.md", VISION),
        ("doc/product/section_01.md", "# Alpha\n"),
        ("doc/product/section_07.md", "# Café\n"),
        (
            "doc/plans/index.md",
            "# Metaplan\n\n## 1. Delivery truth (`PLAN-foo.md`)\n",
        ),
        (
            "doc/plans/IN_PROGRESS-PLAN-foo.md",
            "# Plan\n\nImplements: 01\n",
        ),
    ] {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

fn assert_opening(start: AuthoringStart, step: &str, target: &str, skill: &str) {
    let (_temp, project) = fixture();
    assert_eq!(
        opening_message(&start, &project).unwrap(),
        format!(
            "loom plan: {step}.\n{target}\nLoad the skill loom-plan:{skill}, then begin \
            by telling me what you read and what you plan to ask."
        )
    );
}

#[test]
fn start_vision() {
    assert_opening(
        AuthoringStart::Offer(Offer::StartVision),
        "step 1, Vision",
        "Write doc/product/index.md, the product vision.",
        "vision",
    );
}

#[test]
fn fix_vision_table() {
    assert_opening(
        AuthoringStart::Offer(Offer::FixVisionTable),
        "step 1, Vision",
        "Fix the \"Specification sections\" table in doc/product/index.md so the app can read it.",
        "vision",
    );
}

#[test]
fn add_section_to_vision() {
    assert_opening(
        AuthoringStart::Offer(Offer::AddSectionToVision { number: 7 }),
        "step 2, Product spec",
        "Add doc/product/section_07.md to the \"Specification \
        sections\" table in doc/product/index.md.",
        "vision",
    );
}

#[test]
fn write_section_uses_normalized_full_table_row() {
    assert_opening(
        AuthoringStart::Offer(Offer::WriteSection { number: 7 }),
        "step 2, Product spec",
        "Write doc/product/section_07.md, listed in the vision as \"07 | Café | Pending\".",
        "section",
    );
}

#[test]
fn start_metaplan() {
    assert_opening(
        AuthoringStart::Offer(Offer::StartMetaplan),
        "step 3, Metaplan",
        "Write doc/plans/index.md, the first metaplan.",
        "metaplan",
    );
}

#[test]
fn write_plan_names_entry_heading() {
    assert_opening(
        AuthoringStart::Offer(Offer::WritePlan {
            metaplan: "doc/plans/index.md".into(),
            slug: "foo".into(),
        }),
        "step 4, Plans",
        "Write doc/plans/PLAN-foo.md for the entry \"1. Delivery truth\" \
        in doc/plans/index.md.",
        "plan",
    );
}

#[test]
fn add_plan_to_metaplan() {
    assert_opening(
        AuthoringStart::Offer(Offer::AddPlanToMetaplan { slug: "foo".into() }),
        "step 4, Plans",
        "Add an entry for PLAN-foo.md to a metaplan in doc/plans/.",
        "metaplan",
    );
}

#[test]
fn refine_spec() {
    assert_opening(
        AuthoringStart::Offer(Offer::RefineSpec),
        "done, Review what was built",
        "Review the done plans and their run reviews against the product spec.",
        "retro",
    );
}

#[test]
fn new_metaplan_round_starts_at_two() {
    assert_opening(
        AuthoringStart::Offer(Offer::NewMetaplanRound),
        "done, New metaplan round",
        "Write doc/plans/metaplan-02.md, the next metaplan round.",
        "metaplan",
    );
}

#[test]
fn refine_target() {
    assert_opening(
        AuthoringStart::Refine {
            target: "doc/product/section_07.md".into(),
            passage: false,
        },
        "any, Refine",
        "Refine doc/product/section_07.md.",
        "refine",
    );
}

#[test]
fn refine_passage() {
    assert_opening(
        AuthoringStart::Refine {
            target: "doc/plans/PLAN-foo.md".into(),
            passage: true,
        },
        "any, Refine",
        "Refine doc/plans/PLAN-foo.md. The message that follows attaches \
        the passage.",
        "refine",
    );
}

#[test]
fn message_leaves_opening_text_to_caller() {
    let (_temp, project) = fixture();
    let start = AuthoringStart::Message {
        paths: vec!["doc/plans/index.md".into()],
    };
    assert_eq!(opening_message(&start, &project).unwrap(), "");
}

#[test]
fn knowledge_without_file() {
    assert_opening(
        AuthoringStart::Knowledge { open_file: None },
        "any, Knowledge",
        "Change the knowledge base under doc/loom/knowledge/.",
        "knowledge",
    );
}

#[test]
fn knowledge_with_file() {
    assert_opening(
        AuthoringStart::Knowledge {
            open_file: Some("doc/loom/knowledge/rust.md".into()),
        },
        "any, Knowledge",
        "Change the knowledge base under doc/loom/knowledge/, starting \
        from doc/loom/knowledge/rust.md.",
        "knowledge",
    );
}

#[test]
fn no_step() {
    let (_temp, project) = fixture();
    assert_eq!(
        opening_message(&AuthoringStart::NoStep, &project).unwrap(),
        "loom plan: no step chosen.\nAsk me what I want to work on."
    );
}

#[test]
fn pressure_and_run_have_no_authoring_start() {
    let (_temp, project) = fixture();
    for offer in [Offer::PressureTest { slug: "foo".into() }, Offer::StartRun] {
        assert_eq!(AuthoringStart::from_offer(&offer), None);
        let error = opening_message(&AuthoringStart::Offer(offer.clone()), &project).unwrap_err();
        assert!(error.to_string().contains(&offer.id()));
    }
}

#[test]
fn new_metaplan_round_uses_first_gap() {
    let (temp, _) = fixture();
    for name in ["metaplan-02.md", "metaplan-04.md"] {
        fs::write(temp.path().join("doc/plans").join(name), "# Round\n").unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    assert_eq!(
        opening_message(&AuthoringStart::Offer(Offer::NewMetaplanRound), &project).unwrap(),
        "loom plan: done, New metaplan round.\nWrite doc/plans/metaplan-03.md, the next \
        metaplan round.\nLoad the skill loom-plan:metaplan, then begin by telling me what \
        you read and what you plan to ask."
    );
}

#[test]
fn missing_section_row_names_offer_id() {
    let (_temp, project) = fixture();
    let start = AuthoringStart::Offer(Offer::WriteSection { number: 99 });
    assert!(opening_message(&start, &project)
        .unwrap_err()
        .to_string()
        .contains("write-section:99"));
}

#[test]
fn missing_plan_entry_names_offer_id() {
    let (_temp, project) = fixture();
    for metaplan in ["doc/plans/index.md", "doc/plans/metaplan-02.md"] {
        let start = AuthoringStart::Offer(Offer::WritePlan {
            metaplan: metaplan.into(),
            slug: "missing".into(),
        });
        assert!(opening_message(&start, &project)
            .unwrap_err()
            .to_string()
            .contains("write-plan:missing"));
    }
}

#[test]
fn section_row_truncates_on_character_boundary() {
    let (temp, _) = fixture();
    let vision = format!(
        "# Vision\n\n## Specification sections\n\n| # | Section |\n\
        | --- | --- |\n| 07 | {} |\n",
        "é".repeat(121)
    );
    fs::write(temp.path().join("doc/product/index.md"), vision).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let start = AuthoringStart::Offer(Offer::WriteSection { number: 7 });
    let message = opening_message(&start, &project).unwrap();
    let expected_row = format!("07 | {}…", "é".repeat(114));
    assert_eq!(
        message.lines().nth(1).unwrap(),
        format!("Write doc/product/section_07.md, listed in the vision as \"{expected_row}\".")
    );
}

#[test]
fn section_row_at_limit_has_no_ellipsis() {
    let (temp, _) = fixture();
    let vision = format!(
        "# Vision\n\n## Specification sections\n\n| # | Section |\n\
        | --- | --- |\n| 07 | {} |\n",
        "é".repeat(115)
    );
    fs::write(temp.path().join("doc/product/index.md"), vision).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let start = AuthoringStart::Offer(Offer::WriteSection { number: 7 });
    let message = opening_message(&start, &project).unwrap();
    assert_eq!(
        message.lines().nth(1).unwrap(),
        format!(
            "Write doc/product/section_07.md, listed in the vision as \"07 | {}\".",
            "é".repeat(115)
        )
    );
}

#[test]
fn plan_entry_must_belong_to_selected_metaplan() {
    let (_temp, project) = fixture();
    let start = AuthoringStart::Offer(Offer::WritePlan {
        metaplan: "doc/plans/metaplan-02.md".into(),
        slug: "foo".into(),
    });
    assert!(opening_message(&start, &project)
        .unwrap_err()
        .to_string()
        .contains("write-plan:foo"));
}

#[test]
fn summary_form() {
    assert_eq!(
        summary_opening_message(&["product/section_03".into(), "product".into()]),
        "loom plan: rebuild summaries.\nRebuild: product/section_03, product.\n\
        Load the skill loom-plan:summaries."
    );
}

#[test]
fn pressure_form() {
    assert_eq!(
        pressure_opening_message(PressureCommand::Pressure, "doc/plans/PLAN-foo.md"),
        "/loom-plan:pressure doc/plans/PLAN-foo.md"
    );
}

#[test]
fn address_form() {
    assert_eq!(
        pressure_opening_message(PressureCommand::Address, "doc/plans/PLAN-foo.md"),
        "/loom-plan:address doc/plans/PLAN-foo.md"
    );
}

#[test]
fn authoring_display_names() {
    for (offer, expected) in [
        (Offer::StartVision, "loom plan: vision"),
        (Offer::FixVisionTable, "loom plan: vision"),
        (Offer::AddSectionToVision { number: 7 }, "loom plan: vision"),
        (Offer::WriteSection { number: 7 }, "loom plan: section 07"),
        (Offer::StartMetaplan, "loom plan: metaplan"),
        (
            Offer::WritePlan {
                metaplan: "doc/plans/index.md".into(),
                slug: "foo".into(),
            },
            "loom plan: plan",
        ),
        (
            Offer::AddPlanToMetaplan { slug: "foo".into() },
            "loom plan: metaplan",
        ),
        (Offer::RefineSpec, "loom plan: retro"),
        (Offer::NewMetaplanRound, "loom plan: metaplan"),
    ] {
        assert_eq!(
            display_name("authoring", Some(&AuthoringStart::Offer(offer))),
            expected
        );
    }
}

#[test]
fn other_authoring_display_names() {
    for (start, expected) in [
        (
            AuthoringStart::Refine {
                target: "doc/product/index.md".into(),
                passage: true,
            },
            "loom plan: refine",
        ),
        (
            AuthoringStart::Knowledge { open_file: None },
            "loom plan: knowledge",
        ),
        (
            AuthoringStart::Message { paths: vec![] },
            "loom plan: refine",
        ),
        (AuthoringStart::NoStep, "loom plan: chat"),
    ] {
        assert_eq!(display_name("authoring", Some(&start)), expected);
    }
}

#[test]
fn task_display_names() {
    for (label, expected) in [
        ("summaries", "loom plan: summaries"),
        ("pressure", "loom plan: pressure"),
        ("address", "loom plan: address"),
    ] {
        assert_eq!(display_name(label, None), expected);
    }
}
