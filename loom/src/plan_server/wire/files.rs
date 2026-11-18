//! File-mode wire types: tree, documents, plan views, saves, diffs, search.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileTree {
    pub product: Vec<TreeEntry>,
    pub plans: Vec<TreeEntry>,
    pub knowledge: Vec<TreeEntry>,
    pub other: Vec<TreeEntry>,
    pub archive: Vec<TreeEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEntry {
    /// The owning feature, or null for standalone and unscoped documents.
    pub feature: Option<String>,
    /// Whether a top-level vision, section or metaplan needs a feature home.
    pub misplaced: bool,
    pub path: Option<String>,
    /// For a gap, the repository-relative path of the missing document; null otherwise.
    pub target: Option<String>,
    pub title: String,
    pub file_name: Option<String>,
    pub kind: EntryKind,
    pub state: Option<PlanStateView>,
    pub locked: bool,
    pub gap: bool,
    pub pressure_date: Option<String>,
    pub errors: u32,
    pub blurb: Option<String>,
    pub children: Vec<TreeEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A group of documents sharing a feature directory.
    Feature,
    Vision,
    Section,
    Note,
    Metaplan,
    MetaplanEntry,
    Plan,
    Brief,
    BriefFolder,
    Knowledge,
    KnowledgeFolder,
    Review,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStateView {
    Written,
    Running,
    Done,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocView {
    /// The owning feature, or null for standalone and unscoped documents.
    pub feature: Option<String>,
    /// Whether this document needs to move into a feature directory.
    pub misplaced: bool,
    pub path: String,
    pub title: String,
    pub text: String,
    pub hash: String,
    pub kind: EntryKind,
    pub editable: bool,
    pub lock: Option<LockView>,
    pub sections: Vec<SectionView>,
    pub plan: Option<PlanView>,
    /// When the file last changed on disk (RFC 3339, UTC); null when it cannot be read.
    pub last_change: Option<String>,
    pub too_large: bool,
    pub truncated: bool,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionView {
    pub level: u8,
    pub title: String,
    pub anchor: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockView {
    pub reason: LockKind,
    pub notice: String,
    pub plan: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockKind {
    Running,
    Done,
    Archive,
    BriefOfLockedPlan,
    Knowledge,
    Summaries,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanView {
    pub slug: String,
    pub state: PlanStateView,
    pub implements: Vec<ImplementsLink>,
    pub pressure_date: Option<String>,
    pub last_change: Option<String>,
    pub validation: Option<ValidationView>,
    pub stages: Vec<StageRow>,
    pub yaml: Option<String>,
    pub yaml_error: Option<String>,
    pub metadata_line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementsLink {
    pub number: u32,
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageRow {
    pub id: String,
    pub name: String,
    pub stage_type: String,
    pub summary: Option<String>,
    pub dependencies: Vec<String>,
    pub description: String,
    pub acceptance: Vec<String>,
    pub files: Vec<String>,
    pub line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationView {
    pub valid: bool,
    pub errors: Vec<ValidationItem>,
    pub warnings: ValidationWarnings,
    pub notes: Vec<String>,
    pub metadata: MetadataView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationItem {
    pub message: String,
    pub stage_id: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationWarnings {
    pub structural: Vec<String>,
    pub knowledge: Vec<String>,
    pub sandbox: Vec<String>,
    pub baseline: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataView {
    Present,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum SaveResult {
    Saved {
        hash: String,
        validation: Option<ValidationView>,
    },
    Conflict {
        current_text: String,
        current_hash: String,
    },
    Invalid {
        validation: ValidationView,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffTexts {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub heading_path: Vec<String>,
    pub anchor: String,
    pub snippet: String,
    pub terms: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocChanged {
    pub path: String,
    pub hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationChanged {
    pub path: String,
    pub validation: ValidationView,
}
