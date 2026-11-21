//! The roots a confined command can write, as the toolchain read roots see
//! them. A command can plant a symlink anywhere it writes, so a directory the
//! read roots derive from a path that reaches a writable root (a `PATH` entry
//! in the tree, a symlink in such an entry, a symlink whose resolution passes
//! through the tree) would let one command choose what the next one reads.

use std::path::{Component, Path, PathBuf};

/// Symlink hops followed before a path counts as reaching a writable root;
/// the kernel's own limit is of the same order (`MAXSYMLINKS`).
const MAX_HOPS: u32 = 40;

/// The macOS firmlink prefix: `/System/Volumes/Data/Users/me` is `/Users/me`.
const FIRMLINK: &str = "/System/Volumes/Data";

/// Writable roots as given and resolved through symlinks.
#[derive(Debug, Default)]
pub(super) struct Writable(Vec<PathBuf>);

impl Writable {
    pub(super) fn new(roots: &[&Path]) -> Writable {
        let mut spellings = Vec::new();
        for root in roots {
            spellings.extend(self::spellings(root));
            if let Ok(canonical) = root.canonicalize() {
                spellings.extend(self::spellings(&canonical));
            }
        }
        Writable(spellings)
    }

    /// Whether `path` is, contains or lies inside a writable root, as written
    /// or resolved, or its resolution passes through one at any symlink hop.
    /// A path whose resolution loops or runs past [`MAX_HOPS`] counts too.
    pub(super) fn reached_by(&self, path: &Path) -> bool {
        if self.0.is_empty() {
            return false;
        }
        let mut hops = 0;
        match self.resolve(path, &mut hops) {
            Some(resolved) => self.overlaps(path) || self.overlaps(&resolved),
            None => true,
        }
    }

    /// `path` resolved one component at a time, or `None` once a prefix lies
    /// inside a writable root or the hops run out. A missing component ends
    /// the symlink resolution; the rest is appended as written.
    fn resolve(&self, path: &Path, hops: &mut u32) -> Option<PathBuf> {
        let mut current = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    current.pop();
                }
                Component::CurDir => {}
                other => current.push(other),
            }
            if self.inside(&current) {
                return None;
            }
            if let Ok(target) = std::fs::read_link(&current) {
                *hops += 1;
                if *hops > MAX_HOPS {
                    return None;
                }
                current.pop();
                let target = current.join(target);
                current = self.resolve(&target, hops)?;
            }
        }
        Some(current)
    }

    /// Whether `path` lies inside (or is) a writable root.
    fn inside(&self, path: &Path) -> bool {
        spellings(path)
            .iter()
            .any(|path| self.0.iter().any(|root| path.starts_with(root)))
    }

    /// Whether `path` is, contains or lies inside a writable root.
    fn overlaps(&self, path: &Path) -> bool {
        spellings(path).iter().any(|path| {
            self.0
                .iter()
                .any(|root| path.starts_with(root) || root.starts_with(path))
        })
    }
}

/// `path`, and on its firmlink spelling the path it stands for.
fn spellings(path: &Path) -> Vec<PathBuf> {
    let unfirmlinked = path
        .strip_prefix(FIRMLINK)
        .ok()
        .filter(|rest| !rest.as_os_str().is_empty())
        .map(|rest| Path::new("/").join(rest));
    std::iter::once(path.to_path_buf())
        .chain(unfirmlinked)
        .collect()
}
