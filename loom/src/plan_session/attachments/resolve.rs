//! Checks a browser's attachment against the project and fills in what only the server knows.

use std::path::{Path, PathBuf};

use crate::planning::paths::{check_relpath_syntax, LockReason};
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::slug::PlanState;
use crate::planning::summary::store::{validate_id, STORE_DIR};
use crate::planning::summary::write::read_summary;

use super::parse::is_sentence_id;
use super::stored::{self, NAME_CHARS};
use super::{
    Attachment, AttachmentError, AttachmentInput, AttachmentKind, DocumentAttachmentInput,
    FileAttachmentInput, FileMeta, LockedTag, MAX_QUOTE_BYTES,
};

const KNOWLEDGE_DIR: &str = "doc/loom/knowledge";

/// Resolve one attachment a browser sent: a document against `project`, an uploaded file
/// against the store in `attachments`.
pub fn resolve_input(
    project: &Project,
    attachments: &Path,
    input: AttachmentInput,
) -> Result<Attachment, AttachmentError> {
    match input {
        AttachmentInput::File(file) => resolve_file(attachments, file),
        AttachmentInput::Document(document) => resolve(project, document),
    }
}

/// The attachment of a stored file. The id is the only thing taken from the browser: it must be
/// 16 lowercase hex characters naming a stored file, and the returned `path` is that file's
/// absolute path, for the delivered block only (see [`without_file_paths`]). The name, media
/// type and size come from the store.
pub fn resolve_file(
    attachments: &Path,
    input: FileAttachmentInput,
) -> Result<Attachment, AttachmentError> {
    let unknown = || AttachmentError::UnknownFile(input.id.clone());
    let (path, sidecar) = stored::locate(attachments, &input.id).ok_or_else(unknown)?;
    let path = path
        .to_str()
        .filter(|text| !has_control(text))
        .ok_or_else(unknown)?;
    let name: String = sidecar
        .name
        .chars()
        .filter(|&c| !stored::is_unsafe_name_char(c))
        .collect();
    let name = name.trim().chars().take(NAME_CHARS).collect::<String>();
    let name = if name.is_empty() {
        "attachment".to_owned()
    } else {
        name
    };
    Ok(Attachment {
        kind: AttachmentKind::File,
        path: path.to_owned(),
        anchor: None,
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: String::new(),
        changed_since_selected: false,
        locked: None,
        file: Some(FileMeta {
            id: input.id.clone(),
            name,
            media_type: sidecar.media_type,
            bytes: sidecar.bytes,
        }),
    })
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// `attachments` as browsers receive them: a file chip keeps its `file` and loses the absolute
/// path the delivered block needed.
pub fn without_file_paths(mut attachments: Vec<Attachment>) -> Vec<Attachment> {
    for attachment in attachments
        .iter_mut()
        .filter(|a| a.kind == AttachmentKind::File)
    {
        attachment.path.clear();
    }
    attachments
}

/// Validate `input` and return the attachment to deliver. Nothing in `input` is trusted: the
/// paths must be discovered documents (a summary: a file in the summary store), the summary
/// sentence must match its anchor, and `changed_since_selected`, `locked` and the repo-relative
/// `rests_on` are computed here.
pub fn resolve(
    project: &Project,
    input: DocumentAttachmentInput,
) -> Result<Attachment, AttachmentError> {
    if input.kind == AttachmentKind::File {
        // A document input never names a stored file; those arrive as `AttachmentInput::File`.
        return Err(AttachmentError::NotADocument);
    }
    let rel = repo_path(&input.path).ok_or_else(|| AttachmentError::Path(input.path.clone()))?;
    check_tokens(&input)?;
    check_sentence(&input)?;
    check_rests_on(project, &input.rests_on)?;
    if input.quote.len() > MAX_QUOTE_BYTES {
        return Err(AttachmentError::QuoteTooLarge {
            bytes: input.quote.len(),
        });
    }
    let current = current_hash(project, &input, &rel)?;
    let changed = input
        .selected_hash
        .as_deref()
        .is_some_and(|selected| current.as_deref() != Some(selected));
    // A stage address always reads `stage:<id>`, whatever the browser put in `anchor`.
    let anchor = match &input.stage {
        Some(stage) => Some(format!("stage:{stage}")),
        None => input.anchor,
    };
    Ok(Attachment {
        kind: input.kind,
        path: input.path,
        anchor,
        lines: input.lines,
        stage: input.stage,
        summary_sentence: input.summary_sentence,
        rests_on: input.rests_on,
        quote: input.quote,
        changed_since_selected: changed,
        locked: locked_tag(project, &rel),
        file: None,
    })
}

/// The repo-relative path of a `doc/...` wire path, or `None` when it fails
/// `check_relpath_syntax` or holds a character the block's header cannot carry.
fn repo_path(path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix("doc/")?;
    if rest.contains('#') || rest.chars().any(char::is_whitespace) {
        return None;
    }
    check_relpath_syntax(rest).ok()
}

/// Header fields are separated by spaces and lines: an anchor or stage id with whitespace or a
/// control character would forge fields or lines of the block.
fn is_token(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn check_tokens(input: &DocumentAttachmentInput) -> Result<(), AttachmentError> {
    let mut fields = input.anchor.iter().chain(&input.stage);
    match fields.find(|field| !is_token(field)) {
        Some(field) => Err(AttachmentError::Path(field.clone())),
        None => Ok(()),
    }
}

fn check_sentence(input: &DocumentAttachmentInput) -> Result<(), AttachmentError> {
    let sentence = input.summary_sentence.as_deref();
    let valid = match input.kind {
        AttachmentKind::Source => sentence.is_none(),
        AttachmentKind::File => false,
        AttachmentKind::Summary => {
            sentence == input.anchor.as_deref() && sentence.is_some_and(is_sentence_id)
        }
    };
    valid.then_some(()).ok_or(AttachmentError::SentenceId)
}

/// Every entry is `doc/<path>#<anchor>` of a discovered document, one token without commas
/// (the header joins the entries with commas).
fn check_rests_on(project: &Project, entries: &[String]) -> Result<(), AttachmentError> {
    for entry in entries {
        let bad = || AttachmentError::RestsOn(entry.clone());
        if entry
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',')
        {
            return Err(bad());
        }
        let (path, anchor) = entry.split_once('#').ok_or_else(bad)?;
        let rel = repo_path(path).ok_or_else(bad)?;
        if anchor.is_empty() || project.tree.get(&rel).is_none() {
            return Err(bad());
        }
    }
    Ok(())
}

/// The hash the selection is compared with: `Source` documents carry the project's content
/// hash; a summary file is not discovered, so its text is read no-follow from the store and
/// hashed like `ConceptPage.file_hash`. `None` for a discovered document with no hash.
fn current_hash(
    project: &Project,
    input: &DocumentAttachmentInput,
    rel: &Path,
) -> Result<Option<String>, AttachmentError> {
    match input.kind {
        AttachmentKind::Source => {
            project.tree.get(rel).ok_or(AttachmentError::NotADocument)?;
            Ok(project.content_hash(rel).map(str::to_owned))
        }
        AttachmentKind::Summary => summary_hash(project, &input.path)
            .map(Some)
            .ok_or(AttachmentError::NotADocument),
        AttachmentKind::File => Err(AttachmentError::NotADocument),
    }
}

fn summary_hash(project: &Project, path: &str) -> Option<String> {
    let store = format!("{STORE_DIR}/");
    let id = path.strip_prefix(store.as_str())?.strip_suffix(".md")?;
    validate_id(id).ok()?;
    let text = read_summary(&project.root, id).ok()??;
    Some(normalized_hash(&text))
}

fn locked_tag(project: &Project, rel: &Path) -> Option<LockedTag> {
    match project.locked.lock_reason(rel) {
        Some(LockReason::PlanState(PlanState::Running)) => Some(LockedTag::Running),
        Some(LockReason::PlanState(PlanState::Retired)) => Some(LockedTag::Retired),
        Some(
            LockReason::PlanState(PlanState::Done) | LockReason::Archive | LockReason::RunReview,
        ) => Some(LockedTag::Done),
        Some(LockReason::BriefOfLockedPlan { .. }) => Some(LockedTag::Brief),
        Some(LockReason::PlanState(PlanState::Written)) | None => rel
            .starts_with(KNOWLEDGE_DIR)
            .then_some(LockedTag::Knowledge),
    }
}
