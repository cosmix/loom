//! Wire types for `/api/concept/` and the `ConceptChanged` WebSocket message.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConceptPage {
    /// The owning feature, or null for a standalone plan.
    pub feature: Option<String>,
    pub key: String,
    pub title: String,
    /// Repo-relative path starting with `doc/`.
    pub path: Option<String>,
    pub file_hash: Option<String>,
    pub colour: Option<u8>,
    pub state: Option<SummaryView>,
    pub plan_state: Option<String>,
    pub last_pressure: Option<String>,
    pub key_chips: Vec<KeyChip>,
    pub blocks: Vec<Block>,
    /// The detail level of a two-level summary, which its opening sentences cite; empty and
    /// absent from the JSON for every other page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail: Vec<Sentence>,
    pub unlisted: Vec<DocLink>,
    pub implements: Option<ImplementsView>,
    pub implemented_by: Vec<PlanLink>,
    pub empty: Option<EmptyState>,
    pub building: Option<Progress>,
    pub estimate: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct KeyChip {
    pub doc: DocLink,
    pub colour: u8,
    pub label: String,
    pub out_of_date: Option<OutOfDate>,
    pub no_plan: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DocLink {
    pub id: String,
    pub title: String,
    /// Repo-relative path starting with `doc/`.
    pub path: String,
    pub page: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph { sentences: Vec<Sentence> },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Sentence {
    pub id: String,
    pub text: String,
    pub marks: Vec<Mark>,
    pub citations: Vec<Citation>,
    pub rests_on: Vec<SourceAnchor>,
    pub plans: Vec<PlanLink>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Mark {
    pub doc: DocLink,
    pub colour: u8,
    pub out_of_date: Option<OutOfDate>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Citation {
    Sentence {
        summary: String,
        id: String,
    },
    Source {
        /// Repo-relative path starting with `doc/`.
        path: String,
        anchor: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SourceAnchor {
    /// Repo-relative path starting with `doc/`.
    pub path: String,
    pub anchor: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SummaryView {
    pub state: SummaryStateView,
    pub out_of_date: Option<OutOfDate>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SummaryStateView {
    Current,
    Incomplete,
    Stale,
    Rebuilding,
    Missing,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OutOfDate {
    pub reason: OutOfDateReason,
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutOfDateReason {
    SourceChanged,
    Rebuilding,
    AwaitingApproval,
    CannotRebuild,
    Failed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImplementsView {
    NotStated,
    Sections { links: Vec<SectionLink> },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SectionLink {
    pub number: Option<u32>,
    pub name: String,
    pub doc: Option<DocLink>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PlanLink {
    pub slug: String,
    pub title: String,
    pub state: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct EmptyState {
    pub kind: EmptyKind,
    pub count: Option<u32>,
    pub offer: Option<String>,
    pub docs: Vec<DocLink>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmptyKind {
    NoVision,
    SectionsNoVision,
    NoMetaplan,
    PlansNoMetaplan,
    NeverBuilt,
    Building,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConceptChanged {
    /// The affected feature, or null for standalone documents.
    pub feature: Option<String>,
    pub pages: Vec<String>,
    pub out_of_date: Vec<String>,
    /// Repo-relative paths starting with `doc/`.
    pub sources: Vec<String>,
}
