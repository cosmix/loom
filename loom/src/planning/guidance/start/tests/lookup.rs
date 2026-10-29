use super::*;

#[test]
fn offer_id_lookup_uses_selected_primary_and_secondary() {
    let (_temp, mut project) = fixture();
    let checklist = &mut project.features[0].checklist;
    let primary = Offer::StartMetaplan {
        feature: "f".into(),
    };
    let secondary = Offer::WriteSection {
        feature: "f".into(),
        number: 5,
    };
    checklist.primary = Some(primary.clone());
    checklist.secondary = vec![secondary.clone()];

    for offer in [primary, secondary] {
        assert_eq!(
            AuthoringStart::from_offer_id(&offer.id(), &project, Some("f")),
            Ok(AuthoringStart::Offer(offer))
        );
    }
    assert!(AuthoringStart::from_offer_id("start-metaplan", &project, None).is_err());
    assert!(AuthoringStart::from_offer_id("start-metaplan", &project, Some("other")).is_err());
}

#[test]
fn lookup_preserves_unavailable_offer_errors() {
    let (_temp, mut project) = fixture();
    let offer = Offer::DraftAllSections {
        feature: "f".into(),
    };
    project.features[0].checklist.primary = Some(offer.clone());

    assert_eq!(
        AuthoringStart::from_offer_id(&offer.id(), &project, Some("f")),
        Err(StartError::NotAvailableYet(offer.id()))
    );
}

#[test]
fn move_lookup_returns_file_move_error() {
    let (_temp, mut project) = fixture();
    let offer = Offer::MoveIntoFeature {
        path: "doc/product/index.md".into(),
    };
    project.moves = vec![offer.clone()];

    assert_eq!(
        AuthoringStart::from_offer_id(&offer.id(), &project, None),
        Err(StartError::FileMove(offer.id()))
    );
}

#[test]
fn offer_id_lookup_rejects_unknown_and_non_authoring_offers() {
    let (_temp, mut project) = fixture();
    project.features[0].checklist.primary = Some(Offer::StartRun);
    project.features[0].checklist.secondary = vec![Offer::PressureTest {
        feature: "f".into(),
        slug: "x".into(),
    }];

    for id in [
        "",
        "../x",
        "write-section:99",
        "start-vision",
        "start-run",
        "pressure-test:x",
    ] {
        assert!(
            AuthoringStart::from_offer_id(id, &project, Some("f")).is_err(),
            "{id:?}"
        );
    }
}

#[test]
fn document_start_ids_resolve_to_repo_relative_paths() {
    let (_temp, project) = fixture();
    for (id, expected) in [
        (
            "refine:product/f/section_05.md",
            AuthoringStart::Refine {
                target: "doc/product/f/section_05.md".into(),
                passage: false,
            },
        ),
        (
            "refine-passage:plans/f/index.md",
            AuthoringStart::Refine {
                target: "doc/plans/f/index.md".into(),
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
            AuthoringStart::from_offer_id(id, &project, Some("f")),
            Ok(expected),
            "{id}"
        );
    }
}

#[test]
fn document_start_ids_reject_bad_syntax_and_absent_documents() {
    let (temp, project) = fixture();
    fs::write(
        temp.path().join("doc/product/f/unloaded.md"),
        "# Unloaded\n",
    )
    .unwrap();
    for prefix in ["refine:", "refine-passage:", "knowledge:"] {
        for path in [
            "",
            "../x",
            "/product/f/index.md",
            "product\\index.md",
            "product//f/index.md",
            "product/./f/index.md",
            "product/../f/index.md",
            "product/f/index.md\n",
            "product/f/missing.md",
            "product/f/unloaded.md",
            "product",
            "doc/product/f/index.md",
        ] {
            let id = format!("{prefix}{path}");
            assert!(
                AuthoringStart::from_offer_id(&id, &project, Some("f")).is_err(),
                "{id:?}"
            );
        }
    }
}
