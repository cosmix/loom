use super::*;
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in [
        (
            "product/index.md",
            "# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n| 05 | Concept mode |\n",
        ),
        ("product/section_05.md", "# Concept mode\n"),
        ("plans/index.md", "# Metaplan\n"),
        ("loom/knowledge/index.md", "# Knowledge\n"),
    ] {
        let path = temp.path().join("doc").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

#[test]
fn spec_offers_use_spec_class() {
    for offer in [
        Offer::StartVision,
        Offer::FixVisionTable,
        Offer::AddSectionToVision { number: 5 },
        Offer::WriteSection { number: 5 },
        Offer::RefineSpec,
    ] {
        let start = AuthoringStart::from_offer(&offer).unwrap();
        assert_eq!(step_class(&start), StepClass::Spec, "{offer:?}");
        assert_eq!(start, AuthoringStart::Offer(offer));
    }
}

#[test]
fn plan_offers_use_plan_class() {
    for offer in [
        Offer::StartMetaplan,
        Offer::WritePlan {
            metaplan: "doc/plans/index.md".into(),
            slug: "delivery-truth".into(),
        },
        Offer::AddPlanToMetaplan {
            slug: "delivery-truth".into(),
        },
        Offer::NewMetaplanRound,
    ] {
        let start = AuthoringStart::from_offer(&offer).unwrap();
        assert_eq!(step_class(&start), StepClass::Plan, "{offer:?}");
        assert_eq!(start, AuthoringStart::Offer(offer));
    }
}

#[test]
fn pressure_and_run_are_not_authoring_offers() {
    for offer in [Offer::PressureTest { slug: "x".into() }, Offer::StartRun] {
        assert_eq!(AuthoringStart::from_offer(&offer), None, "{offer:?}");
    }
}

#[test]
fn refinement_class_depends_on_target_directory() {
    for (target, class) in [
        ("doc/product/section_05.md", StepClass::Spec),
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
            vec!["doc/plans/PLAN-x.md", "doc/product/index.md"],
            StepClass::Plan,
        ),
        (
            vec!["doc/product/index.md", "doc/plans/PLAN-x.md"],
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
fn knowledge_and_no_step_use_spec_class() {
    for start in [
        AuthoringStart::Knowledge { open_file: None },
        AuthoringStart::Knowledge {
            open_file: Some("doc/plans/index.md".into()),
        },
        AuthoringStart::NoStep,
    ] {
        assert_eq!(step_class(&start), StepClass::Spec, "{start:?}");
    }
}

#[test]
fn offer_id_lookup_uses_primary_and_secondary_offers() {
    let (_temp, mut project) = fixture();
    project.checklist.primary = Some(Offer::StartMetaplan);
    project.checklist.secondary = vec![Offer::WriteSection { number: 5 }];

    assert_eq!(
        AuthoringStart::from_offer_id("start-metaplan", &project),
        Some(AuthoringStart::Offer(Offer::StartMetaplan)),
    );
    assert_eq!(
        AuthoringStart::from_offer_id("write-section:05", &project),
        Some(AuthoringStart::Offer(Offer::WriteSection { number: 5 })),
    );
    project.checklist.primary = None;
    assert_eq!(
        AuthoringStart::from_offer_id("start-metaplan", &project),
        None
    );
}

#[test]
fn offer_id_lookup_rejects_unknown_and_non_authoring_offers() {
    let (_temp, mut project) = fixture();
    project.checklist.primary = Some(Offer::StartRun);
    project.checklist.secondary = vec![Offer::PressureTest { slug: "x".into() }];

    for id in [
        "",
        "../x",
        "write-section:99",
        "start-vision",
        "start-run",
        "pressure-test:x",
    ] {
        assert_eq!(AuthoringStart::from_offer_id(id, &project), None, "{id:?}");
    }
}

#[test]
fn document_start_ids_resolve_to_repo_relative_paths() {
    let (_temp, project) = fixture();

    for (id, expected) in [
        (
            "refine:product/section_05.md",
            AuthoringStart::Refine {
                target: "doc/product/section_05.md".into(),
                passage: false,
            },
        ),
        (
            "refine-passage:plans/index.md",
            AuthoringStart::Refine {
                target: "doc/plans/index.md".into(),
                passage: true,
            },
        ),
        ("knowledge", AuthoringStart::Knowledge { open_file: None }),
        (
            "knowledge:loom/knowledge/index.md",
            AuthoringStart::Knowledge {
                open_file: Some("doc/loom/knowledge/index.md".into()),
            },
        ),
    ] {
        assert_eq!(
            AuthoringStart::from_offer_id(id, &project),
            Some(expected),
            "{id}"
        );
    }
}

#[test]
fn document_start_ids_reject_bad_syntax_and_absent_documents() {
    let (temp, project) = fixture();
    fs::write(temp.path().join("doc/product/unloaded.md"), "# Unloaded\n").unwrap();

    for prefix in ["refine:", "refine-passage:", "knowledge:"] {
        for path in [
            "",
            "../x",
            "/product/index.md",
            "product\\index.md",
            "product//index.md",
            "product/./index.md",
            "product/../index.md",
            "product/index.md\n",
            "product/missing.md",
            "product/unloaded.md",
            "product",
            "doc/product/index.md",
        ] {
            let id = format!("{prefix}{path}");
            assert_eq!(AuthoringStart::from_offer_id(&id, &project), None, "{id:?}");
        }
    }
}

#[test]
fn offer_labels_name_the_action_and_section() {
    let (_temp, project) = fixture();
    for (offer, expected) in [
        (Offer::StartVision, "Start a vision discussion"),
        (Offer::FixVisionTable, "Fix the vision's section list"),
        (
            Offer::AddSectionToVision { number: 5 },
            "Add section 05 to vision",
        ),
        (
            Offer::WriteSection { number: 5 },
            "Write section 05 — Concept mode",
        ),
        (Offer::WriteSection { number: 99 }, "Write section 99"),
        (Offer::StartMetaplan, "Start a metaplan"),
        (Offer::RefineSpec, "Review what was built"),
        (Offer::NewMetaplanRound, "Start a new metaplan round"),
    ] {
        assert_eq!(AuthoringStart::Offer(offer).label(&project), expected);
    }
}

#[test]
fn plan_offer_labels_name_the_plan() {
    let (_temp, project) = fixture();
    for (offer, expected) in [
        (
            Offer::WritePlan {
                metaplan: "doc/plans/index.md".into(),
                slug: "delivery-truth".into(),
            },
            "Write PLAN-delivery-truth.md",
        ),
        (
            Offer::AddPlanToMetaplan {
                slug: "delivery-truth".into(),
            },
            "Add PLAN-delivery-truth.md to a metaplan",
        ),
    ] {
        assert_eq!(AuthoringStart::Offer(offer).label(&project), expected);
    }
}

#[test]
fn refinement_labels_name_the_target() {
    let (_temp, project) = fixture();
    for (start, expected) in [
        (
            AuthoringStart::Refine {
                target: "doc/plans/PLAN-x.md".into(),
                passage: false,
            },
            "Refine doc/plans/PLAN-x.md",
        ),
        (
            AuthoringStart::Refine {
                target: "doc/product/section_05.md".into(),
                passage: true,
            },
            "Refine doc/product/section_05.md",
        ),
    ] {
        assert_eq!(start.label(&project), expected);
    }
}

#[test]
fn knowledge_labels_name_the_open_file_when_present() {
    let (_temp, project) = fixture();
    let start = AuthoringStart::Knowledge {
        open_file: Some("doc/loom/knowledge/index.md".into()),
    };

    assert_eq!(
        start.label(&project),
        "Change the knowledge base, starting from doc/loom/knowledge/index.md",
    );
    assert_eq!(
        AuthoringStart::Knowledge { open_file: None }.label(&project),
        "Change the knowledge base",
    );
}

#[test]
fn message_and_no_step_labels_are_new_chat() {
    let (_temp, project) = fixture();
    for start in [
        AuthoringStart::Message {
            paths: vec!["doc/plans/index.md".into()],
        },
        AuthoringStart::NoStep,
    ] {
        assert_eq!(start.label(&project), "New chat");
    }
}
