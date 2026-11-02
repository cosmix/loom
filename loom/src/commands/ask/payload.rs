//! The files a question is about: where each may come from, reading them
//! under one byte cap, and the prompt they are wrapped into.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, ensure, Context, Result};

use crate::context::untrusted::inline_safe;
use crate::fs::permissions::state_root::{
    CODEX_AUTH_DENY_READ_PATH, CREDENTIAL_DENY_READ_PATHS, CREDENTIAL_FILE_DENY_READ_PATHS,
};
use crate::fs::safe_read::read_to_string_bounded;

/// Total bytes of file content one question may carry.
pub(crate) const MAX_FILE_BYTES: usize = 400_000;

const CREDENTIAL_NAMES: [&str; 6] = [
    ".netrc",
    ".npmrc",
    ".pgpass",
    ".git-credentials",
    ".credentials.json",
    ".pypirc",
];
const CREDENTIAL_PREFIXES: [&str; 6] = [
    ".env",
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    "credentials",
];
const CREDENTIAL_SUFFIXES: [&str; 4] = [".pem", ".key", ".p12", ".pfx"];

/// One file as the worker sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Document {
    /// The path relative to the project root.
    pub path: String,
    pub text: String,
}

/// Read every path once, in order, refusing a credential, a path that leaves
/// `root`, a symlink, a non-UTF-8 file, and content past [`MAX_FILE_BYTES`] in
/// total. `root` must already be canonical.
pub(crate) fn read_documents(root: &Path, paths: &[PathBuf]) -> Result<Vec<Document>> {
    ensure!(!paths.is_empty(), "no paths to read");
    let mut seen = HashSet::new();
    let mut documents = Vec::new();
    let mut remaining = MAX_FILE_BYTES;
    let home = dirs::home_dir().map(|home| home.canonicalize().unwrap_or(home));
    for arg in paths {
        let (canonical, relative) = locate(root, arg)?;
        ensure!(
            !is_credential(&relative) && !is_sandbox_denied(&canonical, home.as_deref()),
            "{} looks like a credential file; refusing to send it",
            shown(arg)
        );
        if !seen.insert(canonical) {
            continue;
        }
        let text = read_to_string_bounded(root, &relative, remaining).with_context(|| {
            format!(
                "cannot read {} within the {MAX_FILE_BYTES} byte payload cap ({remaining} bytes left)",
                shown(arg)
            )
        })?;
        remaining -= text.len();
        documents.push(Document {
            path: relative.to_string_lossy().into_owned(),
            text,
        });
    }
    Ok(documents)
}

/// Resolve `arg` (relative to the current directory, or absolute) to its
/// canonical form and to its path relative to `root`.
///
/// The relative path comes from the lexically normalised argument (`.` and
/// `..` folded without touching the filesystem), because the no-follow read
/// rejects a `..` component. It must equal the canonical relative path: when
/// they differ, a symlink sits somewhere on the way and the argument is
/// refused, so a link never redirects a read.
pub(crate) fn locate(root: &Path, arg: &Path) -> Result<(PathBuf, PathBuf)> {
    let canonical = arg
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", shown(arg)))?;
    let real = canonical
        .strip_prefix(root)
        .map_err(|_| anyhow!("{} is outside the project", shown(arg)))?;
    let absolute =
        std::path::absolute(arg).with_context(|| format!("cannot make {} absolute", shown(arg)))?;
    let lexical = lexical_normal(&absolute);
    ensure!(
        lexical.strip_prefix(root) == Ok(real),
        "{} goes through a symlink; name the file by its real path",
        shown(arg)
    );
    let relative = real.to_path_buf();
    Ok((canonical, relative))
}

/// Fold `.` and `..` components of an absolute path without reading the
/// filesystem; a `..` at the root stays at the root.
fn lexical_normal(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other.as_os_str()),
        }
    }
    normal
}

/// Whether `canonical` is a path loom's own sandbox denies every session, with
/// `~/` expanded against `home` and a trailing `/**` meaning anything under that
/// directory. With no home, there is nothing to compare against.
fn is_sandbox_denied(canonical: &Path, home: Option<&Path>) -> bool {
    let Some(home) = home else {
        return false;
    };
    CREDENTIAL_DENY_READ_PATHS
        .iter()
        .chain(&CREDENTIAL_FILE_DENY_READ_PATHS)
        .chain(std::iter::once(&CODEX_AUTH_DENY_READ_PATH))
        .any(|entry| {
            let entry = entry.strip_prefix("~/").unwrap_or(entry);
            match entry.strip_suffix("/**") {
                Some(directory) => canonical.starts_with(home.join(directory)),
                None => canonical == home.join(entry),
            }
        })
}

/// A `.git` component anywhere, or a basename that names a secret, ignoring
/// ASCII case. Deliberately broad: `credentials.rs` and `.envrc` are refused.
/// `read_documents` passes the path relative to the project root, so a directory
/// above the root cannot trigger it.
pub(crate) fn is_credential(path: &Path) -> bool {
    let in_git = path
        .components()
        .any(|part| matches!(part, Component::Normal(name) if name.eq_ignore_ascii_case(".git")));
    in_git || path.file_name().is_some_and(is_credential_name)
}

fn is_credential_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_lowercase();
    CREDENTIAL_NAMES.contains(&name.as_str())
        || CREDENTIAL_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || CREDENTIAL_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

fn shown(path: &Path) -> String {
    inline_safe(&path.to_string_lossy())
}

/// Rewrite every `</file`, whatever its ASCII case, as `<\/file`, so neither a
/// file nor its path can close the wrapper.
fn defuse_closing_tag(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in lower.match_indices("</file") {
        out.push_str(&text[last..at]);
        out.push_str("<\\/");
        out.push_str(&text[at + 2..at + 6]);
        last = at + 6;
    }
    out.push_str(&text[last..]);
    out
}

/// The stdin payload: `lead` and a blank line when given, then each file as
/// `<file path="...">` ... `</file>`, then the question. The path is flattened
/// to one line, its backslashes and quotes are escaped, and a `</file` in it or
/// in a file is defused, so nothing can close or forge a wrapper.
pub(crate) fn assemble(lead: Option<&str>, documents: &[Document], question: &str) -> String {
    let mut payload = String::new();
    if let Some(lead) = lead {
        payload.push_str(lead);
        payload.push_str("\n\n");
    }
    for document in documents {
        payload.push_str("<file path=\"");
        // Backslashes first, so the ones defusing and quote escaping add stay single.
        let path = inline_safe(&document.path).replace('\\', "\\\\");
        payload.push_str(&defuse_closing_tag(&path).replace('"', "\\\""));
        payload.push_str("\">\n");
        payload.push_str(&defuse_closing_tag(&document.text));
        if !document.text.ends_with('\n') {
            payload.push('\n');
        }
        payload.push_str("</file>\n");
    }
    payload.push_str("Question: ");
    payload.push_str(question);
    payload.push('\n');
    payload
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
