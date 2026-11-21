//! Where test code lives, as comment-free text: every `.rs` file under `tests/`
//! but its data fixtures; every `src/` file that is a test module, by its name
//! or because a `#[cfg(test)] mod` (or any `mod` inside a test module) declares
//! it; and each `#[cfg(test)]` item inside every other `src/` file.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

/// A stretch of test code: `label` names where it is, `code` is its text with
/// comments and whitespace removed, so a call reads the same however it is
/// formatted and prose about a kill is not a kill.
pub struct Region {
    pub label: String,
    pub code: String,
}

fn rust_files_under(dir: &Path, skip: Option<&Path>, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if Some(path.as_path()) == skip {
            continue;
        }
        if path.is_dir() {
            rust_files_under(&path, skip, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The index just past the literal that opens at `at`: a string (`"`), a raw
/// string (`r#"`), or a char (`'x'`); `None` for a lifetime's quote.
fn literal_end(chars: &[char], at: usize) -> Option<usize> {
    let ident = |i: usize| {
        chars
            .get(i)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
    };
    match chars[at] {
        '"' => {
            let mut i = at + 1;
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            Some(i + 1)
        }
        'r' if !ident(at.wrapping_sub(1))
            || (chars[at - 1] == 'b' && !ident(at.wrapping_sub(2))) =>
        {
            let hashes = chars[at + 1..].iter().take_while(|c| **c == '#').count();
            if chars.get(at + 1 + hashes) != Some(&'"') {
                return None;
            }
            let close: Vec<char> = std::iter::once('"')
                .chain("#".repeat(hashes).chars())
                .collect();
            let body = at + 2 + hashes;
            let rest = chars[body..]
                .windows(close.len())
                .position(|w| w == close.as_slice());
            Some(rest.map_or(chars.len(), |offset| body + offset + close.len()))
        }
        '\'' if chars.get(at + 1) == Some(&'\\') => {
            let close = chars[at + 3..].iter().position(|c| *c == '\'');
            Some(close.map_or(chars.len(), |offset| at + 4 + offset))
        }
        '\'' if chars.get(at + 2) == Some(&'\'') => Some(at + 3),
        _ => None,
    }
}

/// `source` without `//` and (nested) `/* */` comments; literals are kept whole.
pub fn strip_comments(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let next = chars.get(i + 1).copied();
        if chars[i] == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if chars[i] == '/' && next == Some('*') {
            let mut depth = 0usize;
            while i < chars.len() {
                match (chars[i], chars.get(i + 1)) {
                    ('/', Some('*')) => (depth, i) = (depth + 1, i + 2),
                    ('*', Some('/')) => (depth, i) = (depth - 1, i + 2),
                    _ => i += 1,
                }
                if depth == 0 {
                    break;
                }
            }
        } else if let Some(end) = literal_end(&chars, i) {
            let end = end.min(chars.len());
            out.extend(&chars[i..end]);
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `code` without whitespace.
pub fn squeeze(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The index of the bracket closing the one at `open` (`{`, `(` or `[`).
pub fn matching(code: &str, open: usize) -> Option<usize> {
    let (up, down) = match code.as_bytes()[open] {
        b'{' => (b'{', b'}'),
        b'(' => (b'(', b')'),
        _ => (b'[', b']'),
    };
    let mut depth = 0usize;
    for (offset, byte) in code.as_bytes()[open..].iter().enumerate() {
        if *byte == up {
            depth += 1;
        } else if *byte == down {
            depth -= 1;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

fn is_test_cfg(attribute: &str) -> bool {
    let attribute = squeeze(attribute);
    attribute.starts_with("#[cfg(") && attribute.contains("test") && !attribute.contains("not(test")
}

/// Whether `path` is named like a test module.
fn named_like_tests(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let in_tests_dir = path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|d| d == "tests");
    name == "tests.rs" || name.ends_with("_tests.rs") || name.starts_with("tests_") || in_tests_dir
}

/// Move the attributes that open `line` into `attributes` and return what
/// follows them, so `#[cfg(test)] mod x;` on one line still declares `x`. An
/// attribute whose bracket does not close on `line` goes to `pending`, for
/// the caller to join with the next line until it closes.
fn take_attributes<'a>(
    mut line: &'a str,
    attributes: &mut Vec<String>,
    pending: &mut String,
) -> &'a str {
    while line.starts_with("#[") {
        let Some(close) = matching(line, 1) else {
            *pending = line.to_string();
            return "";
        };
        attributes.push(line[..=close].to_string());
        line = line[close + 1..].trim_start();
    }
    line
}

/// The `mod name;` declarations in `file`, each with whether it is
/// `#[cfg(test)]`, resolved to the file that holds the module.
fn declared_modules(file: &Path, code: &str) -> Vec<(PathBuf, bool)> {
    let dir = file.parent().expect("a file has a directory");
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let base = match stem {
        "mod" | "lib" | "main" => dir.to_path_buf(),
        _ => dir.join(stem),
    };
    static DECLARATION: OnceLock<Regex> = OnceLock::new();
    static PATH_ATTR: OnceLock<Regex> = OnceLock::new();
    let declaration =
        DECLARATION.get_or_init(|| Regex::new(r"^(pub(\([^)]*\))?\s+)?mod\s+(\w+)\s*;$").unwrap());
    let path_attr = PATH_ATTR.get_or_init(|| Regex::new(r#"#\[path\s*=\s*"([^"]+)"\]"#).unwrap());
    let (mut found, mut attributes) = (Vec::new(), Vec::<String>::new());
    let mut pending = String::new();
    for raw in code.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let joined;
        let line = if pending.is_empty() {
            raw
        } else {
            joined = format!("{} {raw}", std::mem::take(&mut pending));
            joined.as_str()
        };
        let line = take_attributes(line, &mut attributes, &mut pending);
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = declaration.captures(line) {
            let cfg_test = attributes.iter().any(|a| is_test_cfg(a));
            let explicit = attributes.iter().find_map(|a| path_attr.captures(a));
            let target = match explicit {
                Some(path) => dir.join(&path[1]),
                None if base.join(format!("{}.rs", &caps[3])).is_file() => {
                    base.join(format!("{}.rs", &caps[3]))
                }
                None => base.join(&caps[3]).join("mod.rs"),
            };
            found.push((target, cfg_test));
        }
        attributes.clear();
    }
    found
}

/// The `#[cfg(test)]` items with a body in the squeezed `code`.
fn cfg_test_items(code: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("#[cfg(").map(|i| from + i) {
        let end = matching(code, at + 1).unwrap_or(code.len() - 1);
        from = end + 1;
        if !is_test_cfg(&code[at..=end]) {
            continue;
        }
        let mut item = end + 1;
        while code[item..].starts_with("#[") {
            item = matching(code, item + 1).map_or(code.len(), |close| close + 1);
        }
        // A declaration (`mod name;`, `use ...;`) has no body here.
        let Some(brace) = code[item..].find('{').map(|i| item + i) else {
            continue;
        };
        if code[item..brace].contains(';') {
            continue;
        }
        let close = matching(code, brace).unwrap_or(code.len() - 1);
        items.push(code[item..=close].to_string());
        from = close + 1;
    }
    items
}

/// The `src/` files that are test modules: named like one, declared by a
/// `#[cfg(test)] mod`, or declared by any `mod` inside a test module.
fn test_modules(code: &BTreeMap<PathBuf, String>) -> BTreeSet<PathBuf> {
    let mut test_files: BTreeSet<PathBuf> = code
        .keys()
        .filter(|f| named_like_tests(f))
        .cloned()
        .collect();
    let mut queue: Vec<PathBuf> = code
        .iter()
        .flat_map(|(file, text)| declared_modules(file, text))
        .filter_map(|(target, cfg_test)| cfg_test.then_some(target))
        .chain(test_files.iter().cloned())
        .collect();
    while let Some(file) = queue.pop() {
        if let Some(text) = code.get(&file) {
            test_files.insert(file.clone());
            let children = declared_modules(&file, text)
                .into_iter()
                .map(|(target, _)| target);
            queue.extend(children.filter(|target| !test_files.contains(target)));
        }
    }
    test_files
}

/// Every test region under `crate_dir`'s `src/` and `tests/`.
pub fn test_regions(crate_dir: &Path) -> Vec<Region> {
    let relative = |path: &Path| path.strip_prefix(crate_dir).unwrap().display().to_string();
    let read = |file: &PathBuf| strip_comments(&fs::read_to_string(file).expect("read a file"));
    let mut regions = Vec::new();
    let mut files = Vec::new();
    let tests = crate_dir.join("tests");
    rust_files_under(&tests, Some(&tests.join("fixtures")), &mut files);
    for file in &files {
        let code = squeeze(&read(file));
        regions.push(Region {
            label: relative(file),
            code,
        });
    }

    let mut sources = Vec::new();
    rust_files_under(&crate_dir.join("src"), None, &mut sources);
    let code: BTreeMap<PathBuf, String> = sources.iter().map(|f| (f.clone(), read(f))).collect();
    let test_files = test_modules(&code);
    for (file, text) in &code {
        let squeezed = squeeze(text);
        if test_files.contains(file) {
            regions.push(Region {
                label: relative(file),
                code: squeezed,
            });
            continue;
        }
        for (index, item) in cfg_test_items(&squeezed).into_iter().enumerate() {
            let label = format!("{} (#[cfg(test)] item {})", relative(file), index + 1);
            regions.push(Region { label, code: item });
        }
    }
    regions
}
