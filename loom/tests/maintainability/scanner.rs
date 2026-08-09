use std::fs;
use std::path::{Component, Path, PathBuf};

use super::lexer::sanitize;

pub const FILE_LINE_LIMIT: usize = 400;
pub const FUNCTION_LINE_LIMIT: usize = 50;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SourceMeasurement {
    pub path: String,
    pub lines: usize,
    pub functions: Vec<FunctionMeasurement>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunctionMeasurement {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl FunctionMeasurement {
    pub fn lines(&self) -> usize {
        self.end_line - self.start_line + 1
    }
}

pub fn scan_repository(crate_root: &Path) -> Result<Vec<SourceMeasurement>, String> {
    let mut files = Vec::new();
    for root in ["src", "tests"] {
        collect_rust_files(&crate_root.join(root), &mut files)?;
    }
    files.sort();

    files
        .into_iter()
        .map(|path| scan_file(crate_root, &path))
        .collect()
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read {}: {error}", directory.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to enumerate {}: {error}", directory.display()))?;
    paths.sort();

    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("source scan refuses symlink {}", path.display()));
        }
        if metadata.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn scan_file(crate_root: &Path, path: &Path) -> Result<SourceMeasurement, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let relative = path
        .strip_prefix(crate_root)
        .map_err(|_| format!("{} is outside crate root", path.display()))?;
    Ok(SourceMeasurement {
        path: normalize_path(relative)?,
        lines: source.lines().count(),
        functions: scan_functions(&source)?,
    })
}

fn normalize_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| format!("non-UTF-8 source path: {}", path.display()))?,
            ),
            _ => return Err(format!("non-relative source path: {}", path.display())),
        }
    }
    Ok(parts.join("/"))
}

pub fn scan_functions(source: &str) -> Result<Vec<FunctionMeasurement>, String> {
    let sanitized = sanitize(source)?;
    let line_starts = line_starts(&sanitized);
    let mut functions = Vec::new();
    let mut cursor = 0;

    while let Some(start) = find_keyword(&sanitized, cursor, b"fn") {
        if let Some(function) = parse_function(&sanitized, &line_starts, start)? {
            functions.push(function);
        }
        cursor = start + 2;
    }
    Ok(functions)
}

fn parse_function(
    source: &[u8],
    line_starts: &[usize],
    start: usize,
) -> Result<Option<FunctionMeasurement>, String> {
    let Some((name, after_name)) = parse_function_name(source, start + 2) else {
        return Ok(None);
    };
    let Some(body_start) = find_body_start(source, after_name) else {
        return Ok(None);
    };
    let body_end = find_matching_brace(source, body_start).ok_or_else(|| {
        format!(
            "unclosed function body for {name} starting at line {}",
            line_number(line_starts, start)
        )
    })?;
    Ok(Some(FunctionMeasurement {
        name,
        start_line: line_number(line_starts, start),
        end_line: line_number(line_starts, body_end),
    }))
}

fn parse_function_name(source: &[u8], mut cursor: usize) -> Option<(String, usize)> {
    skip_ascii_whitespace(source, &mut cursor);
    let start = cursor;
    while cursor < source.len()
        && !source[cursor].is_ascii_whitespace()
        && !matches!(source[cursor], b'(' | b'<' | b'{' | b';')
    {
        cursor += 1;
    }
    if cursor == start || matches!(source[start], b'(' | b'$') {
        return None;
    }
    let name = std::str::from_utf8(&source[start..cursor])
        .ok()?
        .to_string();
    Some((name, cursor))
}

fn find_body_start(source: &[u8], mut cursor: usize) -> Option<usize> {
    let (mut parentheses, mut brackets, mut angles) = (0usize, 0usize, 0usize);
    while cursor < source.len() {
        match source[cursor] {
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b'{' if parentheses == 0 && brackets == 0 && angles == 0 => return Some(cursor),
            b';' if parentheses == 0 && brackets == 0 && angles == 0 => return None,
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn find_matching_brace(source: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in source[start..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_keyword(source: &[u8], mut cursor: usize, keyword: &[u8]) -> Option<usize> {
    while cursor + keyword.len() <= source.len() {
        if &source[cursor..cursor + keyword.len()] == keyword
            && is_token_boundary(source, cursor.checked_sub(1))
            && is_token_boundary(source, Some(cursor + keyword.len()))
        {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

fn is_token_boundary(source: &[u8], index: Option<usize>) -> bool {
    index
        .and_then(|value| source.get(value))
        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
}

fn skip_ascii_whitespace(source: &[u8], cursor: &mut usize) {
    while source.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}

fn line_starts(source: &[u8]) -> Vec<usize> {
    std::iter::once(0)
        .chain(
            source
                .iter()
                .enumerate()
                .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
        )
        .collect()
}

fn line_number(line_starts: &[usize], index: usize) -> usize {
    line_starts.partition_point(|start| *start <= index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_handles_multiline_signatures_and_large_functions() {
        let statements = (0..52)
            .map(|index| format!("    let value_{index} = {index};"))
            .collect::<Vec<_>>()
            .join("\n");
        let source = format!("pub async fn oversized(\n    input: usize,\n) -> usize {{\n{statements}\n    input\n}}\n");
        let functions = scan_functions(&source).unwrap();

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "oversized");
        assert!(functions[0].lines() > FUNCTION_LINE_LIMIT);
    }

    #[test]
    fn scanner_ignores_braces_and_fake_functions_in_literals_and_comments() {
        let source = r####"fn real() {
    let raw = r###"} fn fake() {"###;
    let quoted = "} fn fake_too() {";
    let character = '}';
    /* nested { /* fn hidden() {} */ } */
    if true { println!("still real"); }
}
"####;
        let functions = scan_functions(source).unwrap();

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "real");
        assert_eq!(functions[0].lines(), 7);
    }
}
