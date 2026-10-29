use super::*;
use crate::planning::guidance::StartError;

#[test]
fn opening_names_feature_and_its_paths() {
    let (_temp, project) = fixture();
    for (offer, path) in [
        (
            Offer::StartVision {
                feature: "f".into(),
            },
            "doc/product/f/index.md",
        ),
        (
            Offer::WriteSection {
                feature: "f".into(),
                number: 7,
            },
            "doc/product/f/section_07.md",
        ),
        (
            Offer::StartMetaplan {
                feature: "f".into(),
            },
            "doc/plans/f/index.md",
        ),
        (
            Offer::WritePlan {
                feature: "f".into(),
                metaplan: "doc/plans/f/index.md".into(),
                slug: "foo".into(),
            },
            "doc/plans/f/PLAN-foo.md",
        ),
    ] {
        let message = opening_message(&AuthoringStart::Offer(offer), &project, Some("f")).unwrap();
        assert!(message.contains(path), "{message}");
        assert!(
            message.contains("Feature: f.\nChecklist: Vision=Done"),
            "{message}"
        );
    }
}

#[test]
fn feature_b_offer_uses_b_vision_row_and_path() {
    let (temp, _) = fixture();
    let product = temp.path().join("doc/product/b");
    fs::create_dir_all(&product).unwrap();
    fs::write(
        product.join("index.md"),
        "# B\n\n## Specification sections\n\n\
        | # | Section |\n| --- | --- |\n| 07 | Only B |\n",
    )
    .unwrap();
    let project = Project::load(temp.path()).unwrap();
    let offer = Offer::WriteSection {
        feature: "b".into(),
        number: 7,
    };

    let start = AuthoringStart::from_offer(&offer).unwrap();
    let message = opening_message(&start, &project, Some("b")).unwrap();

    assert!(message
        .contains("Write doc/product/b/section_07.md, listed in the vision as \"07 | Only B\"."));
    assert!(message.contains("Feature: b."));
    assert_eq!(start.label(), "Write section 07");
    assert!(!message.contains("Café"));
}

#[test]
fn feature_scope_mismatch_is_a_typed_error() {
    let (_temp, project) = fixture();
    let start = AuthoringStart::Offer(Offer::StartVision {
        feature: "b".into(),
    });

    let error = opening_message(&start, &project, Some("f")).unwrap_err();

    assert_eq!(
        error.downcast_ref::<StartError>(),
        Some(&StartError::FeatureMismatch("start-vision".into()))
    );
}

#[test]
fn metaplan_round_ignores_other_features() {
    let (temp, _) = fixture();
    fs::create_dir_all(temp.path().join("doc/plans/b")).unwrap();
    fs::write(
        temp.path().join("doc/plans/b/metaplan-02.md"),
        "# Other round\n",
    )
    .unwrap();
    let project = Project::load(temp.path()).unwrap();

    let message = opening_message(
        &AuthoringStart::Offer(Offer::NewMetaplanRound),
        &project,
        Some("f"),
    )
    .unwrap();

    assert!(message.contains("Write doc/plans/f/metaplan-02.md"));
}

#[test]
fn feature_offer_requires_scope_and_rejects_unsafe_feature_names() {
    let (_temp, project) = fixture();
    let start = AuthoringStart::Offer(Offer::StartVision {
        feature: "f".into(),
    });
    let error = opening_message(&start, &project, None).unwrap_err();
    assert_eq!(
        error.downcast_ref::<StartError>(),
        Some(&StartError::FeatureRequired)
    );
    for feature in ["../escape", "/tmp/escape", "UPPER", "a/b"] {
        let error = opening_message(&start, &project, Some(feature)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<StartError>(),
            Some(&StartError::InvalidTarget(feature.into()))
        );
    }
}
