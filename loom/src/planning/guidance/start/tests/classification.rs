use super::*;

#[test]
fn spec_offers_use_spec_class() {
    for offer in [
        Offer::StartVision {
            feature: "f".into(),
        },
        Offer::FixVisionTable {
            feature: "f".into(),
        },
        Offer::AddSectionToVision {
            feature: "f".into(),
            number: 5,
        },
        Offer::WriteSection {
            feature: "f".into(),
            number: 5,
        },
        Offer::DraftAllSections {
            feature: "f".into(),
        },
    ] {
        let start = AuthoringStart::from_offer(&offer).unwrap();
        assert_eq!(step_class(&start), StepClass::Spec, "{offer:?}");
        assert_eq!(start, AuthoringStart::Offer(offer));
    }
}

#[test]
fn plan_offers_use_plan_class() {
    for offer in [
        Offer::StartMetaplan {
            feature: "f".into(),
        },
        Offer::WritePlan {
            feature: "f".into(),
            metaplan: "doc/plans/f/index.md".into(),
            slug: "delivery-truth".into(),
        },
        Offer::PlanFollowUps,
        Offer::DraftAllPlans {
            feature: "f".into(),
        },
    ] {
        let start = AuthoringStart::from_offer(&offer).unwrap();
        assert_eq!(step_class(&start), StepClass::Plan, "{offer:?}");
        assert_eq!(start, AuthoringStart::Offer(offer));
    }
}

#[test]
fn file_moves_return_typed_errors() {
    let offer = Offer::MoveIntoFeature {
        path: "doc/product/index.md".into(),
    };
    assert_eq!(
        AuthoringStart::from_offer(&offer),
        Err(StartError::FileMove(offer.id()))
    );
}

#[test]
fn pressure_and_run_are_not_authoring_offers() {
    for offer in [
        Offer::PressureTest {
            feature: "f".into(),
            slug: "x".into(),
        },
        Offer::StartRun,
    ] {
        assert_eq!(
            AuthoringStart::from_offer(&offer),
            Err(StartError::NotAuthoring(offer.id()))
        );
    }
}

#[test]
fn refinement_class_depends_on_target_directory() {
    for (target, class) in [
        ("doc/product/f/section_05.md", StepClass::Spec),
        ("doc/plans/f/PLAN-x.md", StepClass::Plan),
        ("doc/plans/PLAN-x.md", StepClass::Plan),
        ("doc/loom/knowledge/index.md", StepClass::Plan),
        ("doc/product-extra/x.md", StepClass::Plan),
    ] {
        for passage in [false, true] {
            let start = AuthoringStart::Refine {
                target: target.into(),
                passage,
            };
            assert_eq!(step_class(&start), class, "{start:?}");
        }
    }
}

#[test]
fn message_class_depends_only_on_first_attachment() {
    for (paths, class) in [
        (vec![], StepClass::Spec),
        (
            vec!["doc/plans/f/PLAN-x.md", "doc/product/f/index.md"],
            StepClass::Plan,
        ),
        (
            vec!["doc/product/f/index.md", "doc/plans/f/PLAN-x.md"],
            StepClass::Spec,
        ),
        (vec!["doc/loom/knowledge/index.md"], StepClass::Spec),
        (vec!["doc/plans-extra/x.md"], StepClass::Spec),
    ] {
        let start = AuthoringStart::Message {
            paths: paths.into_iter().map(PathBuf::from).collect(),
        };
        assert_eq!(step_class(&start), class, "{start:?}");
    }
}

#[test]
fn kickoff_and_knowledge_use_spec_class() {
    for start in [
        AuthoringStart::Kickoff { small: false },
        AuthoringStart::Kickoff { small: true },
        AuthoringStart::Knowledge { open_file: None },
        AuthoringStart::Knowledge {
            open_file: Some("doc/plans/f/index.md".into()),
        },
    ] {
        assert_eq!(step_class(&start), StepClass::Spec, "{start:?}");
    }
}
