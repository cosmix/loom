use super::files::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let model: T = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(model).unwrap(), expected);
}

macro_rules! fixture_test {
    ($name:ident, $type:ty, $file:literal) => {
        #[test]
        fn $name() {
            assert_fixture::<$type>(include_str!(concat!(
                "../../../../web/src/plan/api/fixtures/files-",
                $file,
                ".json"
            )));
        }
    };
}

fixture_test!(tree_fixture_matches_serde_output, FileTree, "tree");
fixture_test!(doc_fixture_matches_serde_output, DocView, "doc");
fixture_test!(doc_plan_fixture_matches_serde_output, DocView, "doc-plan");
fixture_test!(
    save_saved_fixture_matches_serde_output,
    SaveResult,
    "save-saved"
);
fixture_test!(
    save_conflict_fixture_matches_serde_output,
    SaveResult,
    "save-conflict"
);
fixture_test!(
    save_invalid_fixture_matches_serde_output,
    SaveResult,
    "save-invalid"
);
fixture_test!(diff_fixture_matches_serde_output, DiffTexts, "diff");
fixture_test!(search_fixture_matches_serde_output, SearchResults, "search");
fixture_test!(
    doc_changed_fixture_matches_serde_output,
    DocChanged,
    "doc-changed"
);
fixture_test!(
    validation_fixture_matches_serde_output,
    ValidationChanged,
    "validation"
);

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum TreeMessage {
    TreeChanged(FileTree),
}
fixture_test!(
    tree_changed_fixture_matches_serde_output,
    TreeMessage,
    "tree-changed"
);

fn assert_schema_values<T: Serialize + std::fmt::Debug>(schema: &str, array: &str, values: &[T]) {
    let declaration = format!("export const {array} = [");
    let body = schema
        .split_once(&declaration)
        .expect("missing schema array")
        .1
        .split_once(']')
        .expect("unterminated schema array")
        .0;
    for value in values {
        let wire = serde_json::to_value(value).unwrap();
        let wire = wire.as_str().unwrap();
        assert!(
            body.split(|c: char| c == ',' || c.is_whitespace())
                .any(|part| part == format!("\"{wire}\"")),
            "{array} missing {value:?}"
        );
    }
}

/// The file-mode TypeScript schema; a missing file fails the build, never a test silently.
const SCHEMA_FILES_TS: &str = include_str!("../../../../web/src/plan/api/schema-files.ts");

#[test]
fn entry_kinds_are_in_schema() {
    assert_schema_values(
        SCHEMA_FILES_TS,
        "ENTRY_KINDS",
        &[
            EntryKind::Vision,
            EntryKind::Section,
            EntryKind::Note,
            EntryKind::Metaplan,
            EntryKind::MetaplanEntry,
            EntryKind::Plan,
            EntryKind::Brief,
            EntryKind::BriefFolder,
            EntryKind::Knowledge,
            EntryKind::KnowledgeFolder,
            EntryKind::Review,
            EntryKind::Other,
        ],
    );
}

#[test]
fn plan_states_are_in_schema() {
    assert_schema_values(
        SCHEMA_FILES_TS,
        "PLAN_STATES",
        &[
            PlanStateView::Written,
            PlanStateView::Running,
            PlanStateView::Done,
            PlanStateView::Retired,
        ],
    );
}

#[test]
fn lock_kinds_are_in_schema() {
    assert_schema_values(
        SCHEMA_FILES_TS,
        "LOCK_KINDS",
        &[
            LockKind::Running,
            LockKind::Done,
            LockKind::Archive,
            LockKind::BriefOfLockedPlan,
            LockKind::Knowledge,
            LockKind::Summaries,
        ],
    );
}

#[test]
fn metadata_states_are_in_schema() {
    assert_schema_values(
        SCHEMA_FILES_TS,
        "METADATA_STATES",
        &[MetadataView::Present, MetadataView::Missing],
    );
}

#[test]
fn save_results_are_in_schema() {
    let results = [
        SaveResult::Saved {
            hash: String::new(),
            validation: None,
        },
        SaveResult::Conflict {
            current_text: String::new(),
            current_hash: String::new(),
        },
        SaveResult::Invalid {
            validation: ValidationView {
                valid: false,
                errors: vec![],
                warnings: ValidationWarnings {
                    structural: vec![],
                    knowledge: vec![],
                    sandbox: vec![],
                    baseline: vec![],
                },
                notes: vec![],
                metadata: MetadataView::Missing,
            },
        },
    ];
    let tags: Vec<String> = results
        .iter()
        .map(|result| {
            serde_json::to_value(result).unwrap()["result"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_schema_values(SCHEMA_FILES_TS, "SAVE_RESULTS", &tags);
}

#[test]
fn unknown_fields_are_rejected() {
    let mut doc: Value = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/files-doc.json"
    ))
    .unwrap();
    doc["sections"][0]["extra"] = json!(true);
    assert!(serde_json::from_value::<DocView>(doc).is_err());
    assert!(serde_json::from_value::<SaveResult>(json!({
        "result": "conflict", "current_text": "text", "current_hash": "hash", "extra": true
    }))
    .is_err());
}
