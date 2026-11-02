//! `attachment` rows: context Claude Code rendered into the prompt, such as
//! hook output, skill listings and file reminders. Only a row with a
//! non-empty `rendered` array reached the model; the other kinds
//! (`prompt_snapshot`, `hook_success`, `deferred_tools_record`, ...) are
//! bookkeeping and are skipped.

use serde_json::Value;

use super::transcript_types::{AttachmentEntry, Entry, SYNTHETIC_MODEL};

/// The attachment a row carries, placed after the non-synthetic requests in
/// `entries`. The caller applies the `since` cutoff, so `entries` and the
/// result stay in the same window.
pub(super) fn attachment_entry(value: &Value, entries: &[Entry]) -> Option<AttachmentEntry> {
    if value.get("type").and_then(Value::as_str) != Some("attachment") {
        return None;
    }
    let kind = value.pointer("/attachment/type")?.as_str()?;
    let chars = rendered_chars(value.get("rendered")?.as_array()?);
    if chars == 0 {
        return None;
    }
    Some(AttachmentEntry {
        requests_before: entries
            .iter()
            .filter(|entry| matches!(entry, Entry::Assistant(request) if request.model != SYNTHETIC_MODEL))
            .count(),
        kind: kind.to_owned(),
        chars,
    })
}

fn rendered_chars(rendered: &[Value]) -> usize {
    rendered
        .iter()
        .filter_map(|item| item.get("content")?.as_str())
        .map(|content| content.chars().count())
        .sum()
}
