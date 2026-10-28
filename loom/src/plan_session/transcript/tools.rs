//! Tool summaries, patch counts and result text for transcript tool items.

use std::path::Path;

use serde_json::Value;

const TEXT_BYTES: usize = 64 * 1024;

pub(crate) fn summary(tool: &str, input: &Value, project_root: &Path) -> String {
    let text = match tool {
        "Read" => read_summary(input, project_root),
        "Edit" | "Write" | "MultiEdit" => relative_path(field(input, "file_path"), project_root),
        "NotebookEdit" => relative_path(field(input, "notebook_path"), project_root),
        "Bash" => field(input, "command")
            .lines()
            .next()
            .unwrap_or("")
            .to_owned(),
        "Grep" | "Glob" => search_summary(input, project_root),
        "Agent" | "Task" => field(input, "description").to_owned(),
        "WebFetch" => field(input, "url").to_owned(),
        "WebSearch" => field(input, "query").to_owned(),
        _ => fallback_summary(tool, input),
    };
    text.chars().take(200).collect()
}

fn field<'a>(input: &'a Value, key: &str) -> &'a str {
    input.get(key).and_then(Value::as_str).unwrap_or("")
}

fn relative_path(path: &str, project_root: &Path) -> String {
    let path = Path::new(path);
    let relative = path.strip_prefix(project_root).unwrap_or(path);
    if relative.as_os_str().is_empty() && !path.as_os_str().is_empty() {
        return ".".to_owned();
    }
    relative.to_string_lossy().into_owned()
}

fn read_summary(input: &Value, project_root: &Path) -> String {
    let path = relative_path(field(input, "file_path"), project_root);
    let offset = input.get("offset").and_then(Value::as_u64);
    let limit = input.get("limit").and_then(Value::as_u64);
    if offset.is_none() && limit.is_none() {
        return path;
    }
    let start = offset.unwrap_or(1);
    let end = start.saturating_add(limit.unwrap_or(1).saturating_sub(1));
    format!("{path}:{start}-{end}")
}

fn search_summary(input: &Value, project_root: &Path) -> String {
    let pattern = field(input, "pattern");
    let path = field(input, "path");
    if path.is_empty() {
        return pattern.to_owned();
    }
    let path = relative_path(path, project_root);
    format!("{pattern} in {path}")
}

fn fallback_summary(tool: &str, input: &Value) -> String {
    let Some(value) = input.as_object().and_then(|fields| fields.values().next()) else {
        return tool.to_owned();
    };
    let value = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    format!("{tool} {value}")
}

pub(crate) fn patch_counts(structured_patch: &Value) -> (u32, u32) {
    let mut added = 0u32;
    let mut removed = 0u32;
    for hunk in structured_patch.as_array().into_iter().flatten() {
        for line in hunk
            .get("lines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match line.as_str().and_then(|line| line.as_bytes().first()) {
                Some(b'+') => added = added.saturating_add(1),
                Some(b'-') => removed = removed.saturating_add(1),
                _ => {}
            }
        }
    }
    (added, removed)
}

pub(crate) fn unified_patch(structured_patch: &Value) -> String {
    let mut patch = String::new();
    for hunk in structured_patch.as_array().into_iter().flatten() {
        let Some(lines) = hunk.get("lines").and_then(Value::as_array) else {
            continue;
        };
        let number = |key| hunk.get(key).and_then(Value::as_u64).unwrap_or(0);
        let header = format!(
            "@@ -{},{} +{},{} @@\n",
            number("oldStart"),
            number("oldLines"),
            number("newStart"),
            number("newLines")
        );
        if !append_bounded(&mut patch, &header) {
            break;
        }
        for line in lines.iter().filter_map(Value::as_str) {
            if !append_bounded(&mut patch, line) || !append_bounded(&mut patch, "\n") {
                return patch;
            }
        }
    }
    patch
}

pub(crate) fn result_text(content: &Value) -> (String, bool) {
    let mut text = String::new();
    if let Some(content) = content.as_str() {
        let complete = append_bounded(&mut text, content);
        return (text, !complete);
    }
    for block in content.as_array().into_iter().flatten() {
        if field(block, "type") == "text" && !append_bounded(&mut text, field(block, "text")) {
            return (text, true);
        }
    }
    (text, false)
}

// Stop at the first cut: later blocks must not fill space left by a partial UTF-8 character.
fn append_bounded(output: &mut String, text: &str) -> bool {
    let remaining = TEXT_BYTES.saturating_sub(output.len());
    let mut end = text.len().min(remaining);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    output.push_str(&text[..end]);
    end == text.len()
}

#[cfg(test)]
#[path = "tests_tools.rs"]
mod tests;
