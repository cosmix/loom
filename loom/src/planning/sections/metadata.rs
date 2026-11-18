//! Plan metadata block and stage spans.

use super::{span_of as span, Span};
use crate::plan::parser::extract_yaml_metadata_with_ranges;

/// Locate the metadata block and its stage anchors in the supplied lines.
pub(crate) fn metadata_spans(text: &str, lines: &[&str]) -> (Option<Span>, Vec<(String, Span)>) {
    let Ok(extracted) = extract_yaml_metadata_with_ranges(text) else {
        return (None, Vec::new());
    };
    let block_start = line_at(text, extracted.metadata_block_range.start);
    let block_end = line_at(text, extracted.metadata_block_range.end) + 1;
    let fence_start = line_at(text, extracted.yaml_fence_range.start);
    let fence_end = line_at(text, extracted.yaml_fence_range.end);
    let metadata = Some(span(lines, block_start, block_end));
    let candidates: Vec<_> = (fence_start + 1..fence_end)
        .filter_map(|line| stage_id(lines.get(line)?).map(|(indent, id)| (line, indent, id)))
        .collect();
    let Some(indent) = candidates.iter().map(|(_, indent, _)| *indent).min() else {
        return (metadata, Vec::new());
    };
    let stages: Vec<_> = candidates
        .into_iter()
        .filter(|(_, candidate_indent, _)| *candidate_indent == indent)
        .collect();
    let spans = stages
        .iter()
        .enumerate()
        .map(|(index, (start, _, id))| {
            let end = stages.get(index + 1).map_or(fence_end, |next| next.0);
            (id.clone(), span(lines, *start, end))
        })
        .collect();
    (metadata, spans)
}

fn line_at(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
}

fn stage_id(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let value = trimmed.strip_prefix("- id:")?.trim();
    let value = value.strip_prefix(['\'', '"']).unwrap_or(value);
    let value = value.strip_suffix(['\'', '"']).unwrap_or(value);
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return None;
    }
    Some((indent, value.to_string()))
}
