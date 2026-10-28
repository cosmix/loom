//! Path handling of the planning guard: the path a file-tool call names, its lexical and
//! canonical forms, and the read denies of section 12 matched against both forms.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::GuardPolicy;
use crate::fs::permissions::state_root::resolve_state_root;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::sandbox::read_denies;

/// Pseudo-file trees that reach another path through a magic link (`/proc/self/cwd/...`,
/// `/dev/fd/3`): denied for every tool (px-security-01).
const PSEUDO_ROOTS: [&str; 2] = ["/proc", "/dev/fd"];

/// The absolute, lexically normalised path a file-tool call names. A relative path is joined to
/// the payload's `cwd` (absent: `root`). `Grep` and `Glob` without a `path` search `cwd`.
pub(super) fn tool_path(
    tool: &str,
    input: &Value,
    cwd: Option<&Path>,
    root: &Path,
) -> Result<PathBuf, String> {
    let (field, defaults_to_cwd) = match tool {
        "Edit" | "Write" | "MultiEdit" | "Read" => ("file_path", false),
        "NotebookEdit" => ("notebook_path", false),
        "Grep" | "Glob" => ("path", true),
        _ => return Err(format!("{tool} is not a file tool")),
    };
    let base = cwd.unwrap_or(root);
    if !base.is_absolute() {
        return Err(format!("cwd {} is not absolute", base.display()));
    }
    let raw = match input.get(field) {
        Some(Value::String(text)) if !text.is_empty() => text.as_str(),
        None | Some(Value::Null) if defaults_to_cwd => "",
        Some(Value::String(_)) if defaults_to_cwd => "",
        Some(Value::String(_)) => return Err(format!("tool_input.{field} is empty")),
        Some(_) => return Err(format!("tool_input.{field} is not a string")),
        None => return Err(format!("tool_input.{field} is missing")),
    };
    // The guard never expands `~`; a tool that does would reach the home it names.
    if raw.starts_with('~') {
        return Err(format!("{raw} starts with ~"));
    }
    let joined = base.join(raw);
    let normal = normalize(&joined)?;
    if pseudo(&normal) {
        return Err(format!("{} is under /proc or /dev/fd", normal.display()));
    }
    // `..` after a symlink resolves physically somewhere the lexical form does not name.
    let dotdot = joined.components().any(|part| part == Component::ParentDir);
    if dotdot && canonical_of(&joined)? != canonical_of(&normal)? {
        return Err(format!("{} climbs out of a symlink", joined.display()));
    }
    Ok(normal)
}

/// Lexical normalisation of an absolute path: `.` dropped, `..` pops a component, a `..` above
/// `/` is an error.
pub(super) fn normalize(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{} is not absolute", path.display()));
    }
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::RootDir => out.push(Component::RootDir),
            Component::CurDir => {}
            Component::ParentDir => {
                if out.parent().is_none() {
                    return Err(format!("{} climbs above /", path.display()));
                }
                out.pop();
            }
            Component::Normal(name) => out.push(name),
            Component::Prefix(_) => return Err(format!("{} has a prefix", path.display())),
        }
    }
    Ok(out)
}

/// `path` equals `base` or lies below it, compared by component: `/a/bc` is not inside `/a/b`.
pub(super) fn inside(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

/// `path` lies in a pseudo-file tree of [`PSEUDO_ROOTS`].
pub(super) fn pseudo(path: &Path) -> bool {
    PSEUDO_ROOTS
        .iter()
        .any(|root| inside(path, Path::new(root)))
}

/// The physical form of an absolute `path`: canonicalized when it exists, else its deepest
/// existing ancestor canonicalized with the missing tail joined lexically. An existing path
/// whose canonicalization fails (a dangling or looping link) and any lstat error other than
/// "not found" are errors: the guard cannot tell what the tool would open.
pub(super) fn canonical_of(path: &Path) -> Result<PathBuf, String> {
    let mut tail = Vec::new();
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(_) => {
                let mut out = fs::canonicalize(current)
                    .map_err(|e| format!("cannot resolve {}: {e}", current.display()))?;
                out.extend(tail.iter().rev());
                return normalize(&out);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let last = current.components().next_back();
                let (Some(last), Some(parent)) = (last, current.parent()) else {
                    return Err(format!("cannot resolve {}", path.display()));
                };
                tail.push(last.as_os_str().to_owned());
                current = parent;
            }
            Err(error) => return Err(format!("cannot inspect {}: {error}", current.display())),
        }
    }
}

/// One read-denied path in one of its forms.
struct Denied {
    path: PathBuf,
    /// The settings entry it came from, for the deny reason.
    entry: String,
    /// A credential path or the planning runtime root: a recursive search above it is denied.
    /// The project's state-root files are not: a plain search of the project root may pass
    /// above them, and `search::check_search` refuses every search that would walk into the
    /// state root.
    guards_searches: bool,
}

/// The read denies of one session, recomputed by `sandbox::read_denies` so layer 1 and layer 3
/// deny the same paths, each entry in its lexical and canonical forms.
pub(super) struct ReadRules {
    denied: Vec<Denied>,
    /// The session's `allowRead` grants; they win over the denies.
    carve_outs: Vec<PathBuf>,
}

impl ReadRules {
    /// Fails when the denies cannot be computed (a path that is not valid UTF-8); the caller
    /// denies with that reason.
    pub(super) fn new(policy: &GuardPolicy) -> Result<Self, String> {
        let home = &policy.home;
        let reads = read_denies(home, &policy.project_root, &policy.runtime_dir)
            .map_err(|error| format!("cannot compute the read denies: {error:#}"))?;
        let state_root = resolve_state_root(&policy.project_root);
        let mut denied = Vec::new();
        for entry in &reads.denied {
            let Some(path) = rule_path(entry, home) else {
                continue;
            };
            let guards_searches = !state_root.as_deref().is_some_and(|s| inside(&path, s));
            let canonical = fs::canonicalize(&path).ok().filter(|c| *c != path);
            for form in std::iter::once(path).chain(canonical) {
                denied.push(Denied {
                    path: form,
                    entry: entry.clone(),
                    guards_searches,
                });
            }
        }
        let mut grants: Vec<PathBuf> = reads
            .carve_outs
            .iter()
            .filter_map(|entry| rule_path(entry, home))
            .collect();
        if matches!(policy.kind, SessionKind::Pressure { .. }) {
            grants.push(SessionDir::new(policy.session_dir.clone()).done());
        }
        let carve_outs = grants.iter().flat_map(|grant| grant_forms(grant)).collect();
        Ok(ReadRules { denied, carve_outs })
    }

    /// Deny a read of `path` (absolute, normalised) when its lexical or canonical form is
    /// read-denied. A search (`Grep`, `Glob`) is also denied when its root is a symlink, which
    /// ripgrep follows when named on its command line, or a strict ancestor of a credential
    /// path or the planning runtime root, which a recursive search would read inside. The
    /// project's state root is `search::check_search`'s.
    pub(super) fn check(&self, path: &Path, search: bool) -> Result<(), String> {
        let canonical = canonical_of(path)?;
        for form in [path, canonical.as_path()] {
            if pseudo(form) {
                return Err(format!(
                    "{} resolves under /proc or /dev/fd",
                    path.display()
                ));
            }
            if let Some(denied) = self.denied_at(form) {
                return Err(format!(
                    "{} is read-denied ({})",
                    path.display(),
                    denied.entry
                ));
            }
        }
        if !search {
            return Ok(());
        }
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(format!("the search root {} is a symlink", path.display()));
        }
        for form in [path, canonical.as_path()] {
            let above = self.denied.iter().find(|denied| {
                denied.guards_searches && denied.path != form && inside(&denied.path, form)
            });
            if let Some(denied) = above {
                return Err(format!(
                    "a search of {} reaches the read-denied {}",
                    path.display(),
                    denied.entry
                ));
            }
        }
        Ok(())
    }

    /// The deny `form` lies in, unless one of the session's grants covers it.
    fn denied_at(&self, form: &Path) -> Option<&Denied> {
        if self.carve_outs.iter().any(|grant| inside(form, grant)) {
            return None;
        }
        self.denied.iter().find(|denied| inside(form, &denied.path))
    }
}

/// The path a `denyRead`/`allowRead` entry names: `~/` expanded with `home`, a leading `//`
/// collapsed to `/` (the `token_deny_paths` form), a trailing `/**` meaning the directory and
/// everything in it. `None` for an entry that is not absolute after expansion.
fn rule_path(entry: &str, home: &Path) -> Option<PathBuf> {
    let entry = entry.strip_suffix("/**").unwrap_or(entry);
    let path = match entry.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(entry),
    };
    normalize(&path).ok()
}

/// A grant in its lexical form and with its parent canonicalized. The grant's own last
/// component is never followed: a grant directory swapped for a symlink must not carry the
/// carve-out to wherever the link points.
fn grant_forms(grant: &Path) -> Vec<PathBuf> {
    let mut forms = vec![grant.to_path_buf()];
    let resolved = grant
        .parent()
        .zip(grant.file_name())
        .and_then(|(parent, name)| Some(fs::canonicalize(parent).ok()?.join(name)));
    forms.extend(resolved.filter(|form| *form != grant));
    forms
}
