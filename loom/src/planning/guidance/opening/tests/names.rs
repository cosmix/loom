use super::*;

#[test]
fn summary_form() {
    assert_eq!(
        summary_opening_message(&["f/product/section_03".into(), "overview/f/product".into()]),
        "loom plan: rebuild summaries.\nRebuild: f/product/section_03, overview/f/product.\n\
        Load the skill loom-plan:summaries."
    );
}

#[test]
fn pressure_form() {
    assert_eq!(
        pressure_opening_message(PressureCommand::Pressure, "doc/plans/f/PLAN-foo.md"),
        "/loom-plan:pressure doc/plans/f/PLAN-foo.md"
    );
}

#[test]
fn address_form() {
    assert_eq!(
        pressure_opening_message(PressureCommand::Address, "doc/plans/f/PLAN-foo.md"),
        "/loom-plan:address doc/plans/f/PLAN-foo.md"
    );
}

fn authoring_display_name_cases() -> [(Offer, &'static str); 6] {
    [
        (
            Offer::StartVision {
                feature: "f".into(),
            },
            "loom plan: vision",
        ),
        (
            Offer::FixVisionTable {
                feature: "f".into(),
            },
            "loom plan: vision",
        ),
        (
            Offer::AddSectionToVision {
                feature: "f".into(),
                number: 7,
            },
            "loom plan: vision",
        ),
        (
            Offer::WriteSection {
                feature: "f".into(),
                number: 7,
            },
            "loom plan: section 07",
        ),
        (
            Offer::StartMetaplan {
                feature: "f".into(),
            },
            "loom plan: metaplan",
        ),
        (
            Offer::WritePlan {
                feature: "f".into(),
                metaplan: "doc/plans/f/index.md".into(),
                slug: "foo".into(),
            },
            "loom plan: plan",
        ),
    ]
}

#[test]
fn authoring_display_names() {
    let cases = authoring_display_name_cases()
        .into_iter()
        .chain([(Offer::PlanFollowUps, "loom plan: metaplan")]);
    for (offer, expected) in cases {
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
                target: "doc/product/f/index.md".into(),
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
