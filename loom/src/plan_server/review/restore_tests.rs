//! Tests for the restore decision.

use super::restore::*;

fn existing_pin() -> PinnedFile {
    PinnedFile {
        start_text: Some("old".to_owned()),
        end_hash: Some("sha256:e".to_owned()),
    }
}

#[test]
fn deleted_file_refused_without_save() {
    let pin = PinnedFile {
        start_text: Some("old".to_owned()),
        end_hash: None,
    };

    let result = restore_file(
        "plans/PLAN-x.md",
        Some(&pin),
        false,
        true,
        &mut |_, _, _, _| {
            panic!("a deleted file must never reach save, even with force");
        },
    );

    assert_eq!(
        result,
        RestoreResult::Refused("the turn created or deleted this file; ask in chat".to_owned())
    );
}

#[test]
fn invalid_maps_to_invalid() {
    let pin = existing_pin();

    let result = restore_file(
        "plans/PLAN-x.md",
        Some(&pin),
        false,
        false,
        &mut |_, _, _, _| SaveResult::Invalid {
            errors: vec!["missing title".to_owned(), "invalid metadata".to_owned()],
        },
    );

    assert_eq!(
        result,
        RestoreResult::Invalid {
            errors: vec!["missing title".to_owned(), "invalid metadata".to_owned()],
        }
    );
}

#[test]
fn refused_maps_to_refused() {
    let pin = existing_pin();

    let result = restore_file(
        "plans/PLAN-x.md",
        Some(&pin),
        false,
        false,
        &mut |_, _, _, _| SaveResult::Refused("authoring is held by another browser".to_owned()),
    );

    assert_eq!(
        result,
        RestoreResult::Refused("authoring is held by another browser".to_owned())
    );
}

#[test]
fn save_path_of_strips_doc_prefix() {
    assert_eq!(save_path_of("doc/plans/PLAN-x.md"), Some("plans/PLAN-x.md"));
    assert_eq!(
        save_path_of("doc/product/section_01.md"),
        Some("product/section_01.md")
    );
}

#[test]
fn save_path_of_refuses_outside_doc() {
    assert_eq!(save_path_of("planning-omissions.md"), None);
    assert_eq!(save_path_of("docs/plans/PLAN-x.md"), None);
    assert_eq!(save_path_of("/doc/plans/PLAN-x.md"), None);
}

#[test]
fn save_path_of_refuses_invalid_doc_paths() {
    for path in [
        "doc/",
        "doc/../x",
        "doc/plans/./PLAN-x.md",
        "doc//plans/PLAN-x.md",
        "doc/plans/",
        "doc/plans\\PLAN-x.md",
        "doc/plans/PLAN-\0x.md",
    ] {
        assert_eq!(save_path_of(path), None, "invalid save path: {path:?}");
    }
}

#[test]
fn missing_pin_refused_without_save() {
    let result = restore_file("plans/PLAN-x.md", None, false, false, &mut |_, _, _, _| {
        panic!("a missing pin must never reach save");
    });

    assert_eq!(
        result,
        RestoreResult::Refused("the turn created or deleted this file; ask in chat".to_owned())
    );
}

#[test]
fn running_turn_refusal_precedes_missing_pin() {
    let result = restore_file("plans/PLAN-x.md", None, true, true, &mut |_, _, _, _| {
        panic!("a running turn must never reach save, even with force");
    });

    assert_eq!(
        result,
        RestoreResult::Refused("a turn is running".to_owned())
    );
}

#[test]
fn force_forwarded_with_empty_start_text() {
    let pin = PinnedFile {
        start_text: Some(String::new()),
        end_hash: Some("sha256:e".to_owned()),
    };
    let mut calls = Vec::new();

    let result = restore_file(
        "plans/PLAN-x.md",
        Some(&pin),
        false,
        true,
        &mut |path, text, hash, force| {
            calls.push((path.to_owned(), text.to_owned(), hash.to_owned(), force));
            SaveResult::Saved
        },
    );

    assert_eq!(result, RestoreResult::Restored);
    assert_eq!(
        calls,
        vec![(
            "plans/PLAN-x.md".to_owned(),
            String::new(),
            "sha256:e".to_owned(),
            true
        )]
    );
}
