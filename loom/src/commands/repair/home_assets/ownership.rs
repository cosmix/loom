//! Only current shipped bytes (or their pre-prefix frontmatter name) prove
//! that an unprefixed asset belongs to Loom. Anything else is operator data.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::assets::{CLAUDE_AGENTS, SKILLS};

fn legacy_bytes(body: &str, old_name: &str, prefixed_name: &str) -> Vec<u8> {
    body.replacen(
        &format!("name: {prefixed_name}\n"),
        &format!("name: {old_name}\n"),
        1,
    )
    .into_bytes()
}

fn matches_shipped(actual: &[u8], shipped: &str, old_name: &str, prefixed_name: &str) -> bool {
    let legacy = legacy_bytes(shipped, old_name, prefixed_name);
    actual == shipped.as_bytes() || actual == legacy.as_slice()
}

pub(super) fn agent(path: &Path, name: &str) -> bool {
    let prefixed = format!("loom-{name}.md");
    let old = format!("{name}.md");
    let Some((_, body)) = CLAUDE_AGENTS.iter().find(|(key, _)| *key == prefixed) else {
        return false;
    };
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
        && fs::read(path)
            .is_ok_and(|bytes| matches_shipped(&bytes, body, name, &prefixed[..prefixed.len() - 3]))
        && path.with_file_name(prefixed).is_file()
        && path.with_file_name(old) == path
}

pub(super) fn skill(path: &Path, name: &str) -> bool {
    let prefixed = format!("loom-{name}");
    if !path.with_file_name(&prefixed).is_dir() {
        return false;
    }
    let expected: BTreeMap<PathBuf, &str> = SKILLS
        .iter()
        .filter_map(|(key, body)| {
            key.strip_prefix(&format!("{prefixed}/"))
                .map(|relative| (PathBuf::from(relative), *body))
        })
        .collect();
    if expected.is_empty() {
        return false;
    }
    let mut actual = BTreeMap::new();
    if !collect_files(path, path, &mut actual) || actual.len() != expected.len() {
        return false;
    }
    expected.iter().all(|(relative, body)| {
        actual
            .get(relative)
            .is_some_and(|bytes| matches_shipped(bytes, body, name, &prefixed))
    })
}

fn collect_files(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> bool {
    if !fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir()) {
        return false;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        if kind.is_dir() {
            if !collect_files(root, &path, files) {
                return false;
            }
        } else if kind.is_file() {
            let Ok(bytes) = fs::read(&path) else {
                return false;
            };
            let Ok(relative) = path.strip_prefix(root) else {
                return false;
            };
            files.insert(relative.to_path_buf(), bytes);
        } else {
            return false;
        }
    }
    true
}
