use super::*;

#[test]
fn vision_offer_labels_name_the_action_and_section() {
    for (offer, expected) in [
        (
            Offer::StartVision {
                feature: "f".into(),
            },
            "Start the vision",
        ),
        (
            Offer::FixVisionTable {
                feature: "f".into(),
            },
            "Fix the section table",
        ),
        (
            Offer::AddSectionToVision {
                feature: "f".into(),
                number: 5,
            },
            "Add section 05 to the vision",
        ),
        (
            Offer::WriteSection {
                feature: "f".into(),
                number: 5,
            },
            "Write section 05",
        ),
        (
            Offer::WriteSection {
                feature: "f".into(),
                number: 99,
            },
            "Write section 99",
        ),
    ] {
        assert_eq!(AuthoringStart::Offer(offer).label(), expected);
    }
}

#[test]
fn metaplan_and_review_offer_labels_name_the_action() {
    for (offer, expected) in [
        (
            Offer::StartMetaplan {
                feature: "f".into(),
            },
            "Start the metaplan",
        ),
        (Offer::PlanFollowUps, "Plan follow-ups"),
        (Offer::ReviewFeature, "Review the feature"),
    ] {
        assert_eq!(AuthoringStart::Offer(offer).label(), expected);
    }
}

#[test]
fn plan_labels_name_the_plan() {
    let start = AuthoringStart::Offer(Offer::WritePlan {
        feature: "f".into(),
        metaplan: "doc/plans/f/index.md".into(),
        slug: "delivery-truth".into(),
    });
    assert_eq!(start.label(), "Write plan delivery-truth");
}

#[test]
fn refinement_labels_name_the_target() {
    for passage in [false, true] {
        let start = AuthoringStart::Refine {
            target: "doc/product/f/section_05.md".into(),
            passage,
        };
        assert_eq!(start.label(), "Refine doc/product/f/section_05.md");
    }
}

#[test]
fn knowledge_labels_name_the_open_file_when_present() {
    let start = AuthoringStart::Knowledge {
        open_file: Some("doc/loom/knowledge/index.md".into()),
    };
    assert_eq!(
        start.label(),
        "Change the knowledge base, starting from doc/loom/knowledge/index.md"
    );
    assert_eq!(
        AuthoringStart::Knowledge { open_file: None }.label(),
        "Change the knowledge base"
    );
}

#[test]
fn message_label_is_new_chat() {
    let start = AuthoringStart::Message {
        paths: vec!["doc/plans/f/index.md".into()],
    };
    assert_eq!(start.label(), "New chat");
}
