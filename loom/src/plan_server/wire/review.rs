//! Wire bodies and WebSocket messages for the review loop.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::concept::PlanLink;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RestoreRequest {
    pub session: String,
    pub turn: String,
    /// Repository-relative paths starting with `doc/`.
    pub paths: Vec<String>,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RestoreResponse {
    pub files: Vec<RestoreFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RestoreFile {
    /// Repository-relative path starting with `doc/`.
    pub path: String,
    #[serde(flatten)]
    pub result: RestoreFileResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum RestoreFileResult {
    Restored,
    Refused {
        reason: String,
    },
    Changed {
        current_text: String,
        current_hash: String,
        turn_start_text: String,
    },
    Invalid {
        errors: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HashesResponse {
    pub head: Option<String>,
    /// Keys are repository-relative paths starting with `doc/`.
    pub docs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefResponse {
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompareResponse {
    pub commit: String,
    pub available: bool,
    pub docs: Vec<ChangedDocWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChangedDocWire {
    /// Repository-relative path starting with `doc/`.
    pub path: String,
    pub change: DocChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocChange {
    Added,
    Changed,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AtCommitResponse {
    /// Repository-relative path starting with `doc/`.
    pub path: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BuiltResponse {
    pub notes: Vec<BuiltNoteWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuiltNoteWire {
    ChangedAfter {
        slug: String,
        built: String,
        base_commit: Option<String>,
    },
    Running {
        slug: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OpenQuestionsMessage {
    /// Keys are repository-relative paths starting with `doc/`.
    pub counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TurnSectionsResponse {
    pub turn: String,
    pub files: Vec<TurnSectionsFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TurnSectionsFile {
    /// Repository-relative path starting with `doc/`.
    pub path: String,
    pub anchors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImplementedByResponse {
    /// Keys are feature-qualified document ids such as `product/demo/section_02`.
    pub sections: BTreeMap<String, Vec<PlanLink>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SuggestedPassageWire {
    /// Repository-relative path starting with `doc/`.
    pub path: String,
    pub anchor: String,
    pub text: String,
    pub lines: Option<(u32, u32)>,
    pub stage: Option<String>,
    pub summary_sentence: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionActionWire {
    Attach,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewCommand {
    Suggest {
        passage: SuggestedPassageWire,
        note: String,
    },
    SuggestionAnswer {
        id: String,
        action: SuggestionActionWire,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewMessage {
    Suggestion {
        id: String,
        from: String,
        passage: SuggestedPassageWire,
        note: String,
    },
    SuggestionStatus {
        id: String,
        #[serde(flatten)]
        status: SuggestionStatus,
    },
    OpenQuestions {
        /// Keys are repository-relative paths starting with `doc/`.
        counts: BTreeMap<String, usize>,
    },
    Refused {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SuggestionStatus {
    Sent { holder: String, away: bool },
    Attached,
    Dismissed,
    Dropped { reason: String },
}
