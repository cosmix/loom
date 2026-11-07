//! Whether the runner collects a test file at head, for the cross-file move
//! check: a moved test that no runner picks up is a weakened test.

use std::collections::HashMap;

use super::moves::FileDelta;
use crate::verify::contracts::collection::rust::{chain_problems, parent, path_modules};
use crate::verify::contracts::collection::FileSource;

/// Answers "does the runner collect this destination file at head", reading
/// the head tree through `head` and remembering each Rust verdict per file.
pub(super) struct Collector<'a> {
    head: FileSource<'a>,
    rust: HashMap<String, bool>,
}

impl<'a> Collector<'a> {
    pub(super) fn new(head: FileSource<'a>) -> Self {
        Self {
            head,
            rust: HashMap::new(),
        }
    }

    /// Whether the runner collects the test function `scope` (module path
    /// inside the file, function name last) of `dest`.
    ///
    /// Rust follows the `mod` chain from a crate root or `tests/` target
    /// (`chain_problems`), plus one `#[path = "<rel>"]` hop from a file that
    /// chain reaches. Python and JavaScript use the runners' default file
    /// names (pytest, vitest and jest); every other profile's file globs are
    /// its runner's collection, and every delta already matched one.
    pub(super) fn collected(&mut self, dest: &FileDelta, scope: &[String]) -> bool {
        match dest.profile.name {
            "rust" => self.rust_collected(dest, scope),
            "python" => python_name(file_name(&dest.path)),
            "javascript" => javascript_name(file_name(&dest.path)),
            _ => true,
        }
    }

    fn rust_collected(&mut self, dest: &FileDelta, scope: &[String]) -> bool {
        let tail = scope.join("::");
        if tail.is_empty() {
            return false;
        }
        // The chain check compares `tail` only as a proper extension of the
        // file's module path, so the verdict is the same for every function
        // of the file.
        if let Some(known) = self.rust.get(&dest.path) {
            return *known;
        }
        let verdict = self.reached(&dest.path, &tail) || self.through_path_attribute(dest, &tail);
        self.rust.insert(dest.path.clone(), verdict);
        verdict
    }

    /// Some test target's `mod` chain reaches `file` with `tail` (the
    /// function's path below the file's module) as the test name. The root's
    /// directory is unknown here, so every suffix of the file's path serves
    /// as the module path in turn.
    fn reached(&self, file: &str, tail: &str) -> bool {
        let components = module_components(file);
        (0..=components.len()).any(|skip| {
            let mut test = components[skip..].join("::");
            if !test.is_empty() {
                test.push_str("::");
            }
            test.push_str(tail);
            chain_problems(self.head, file, &test).is_empty()
        })
    }

    fn through_path_attribute(&self, dest: &FileDelta, tail: &str) -> bool {
        dest.siblings
            .iter()
            .filter(|declaring| **declaring != dest.path)
            .any(|declaring| {
                let Some(text) = (self.head)(declaring) else {
                    return false;
                };
                path_modules(&text).into_iter().any(|(name, relative)| {
                    resolve(parent(declaring), &relative).as_deref() == Some(dest.path.as_str())
                        && self.reached(declaring, &format!("{name}::{tail}"))
                })
            })
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn python_name(name: &str) -> bool {
    (name.starts_with("test_") || name.ends_with("_test.py")) && name.ends_with(".py")
}

/// `*.test.*` and `*.spec.*`.
fn javascript_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() >= 3
        && parts[1..parts.len() - 1]
            .iter()
            .any(|part| matches!(*part, "test" | "spec"))
}

/// `src/a/b.rs` as `["src", "a", "b"]`, `src/a/mod.rs` as `["src", "a"]`.
fn module_components(file: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = file.trim_end_matches(".rs").split('/').collect();
    if parts.last() == Some(&"mod") {
        parts.pop();
    }
    parts
}

/// `relative` joined to `dir`, `.` and `..` resolved; `None` when it is
/// absolute or climbs above the root.
fn resolve(dir: &str, relative: &str) -> Option<String> {
    if relative.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
#[path = "moves_collect_tests.rs"]
mod tests;
