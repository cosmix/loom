//! `TI-decl-<lang>` and `TI-assert-<lang>`: a language's test declarations or
//! assertions, summed line by line over its test files, fell below the total
//! at base.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use super::{current_sha256, EventKind, IntegrityEvent};
use crate::fs::safe_read::read_bounded;
use crate::testrun::languages::{self, LanguageProfile};
use crate::verify::contracts::changes::git;
use crate::verify::review::fingerprint::ChangeFingerprint;

const MAX_TEST_FILE_BYTES: usize = 10 * 1024 * 1024;

/// A language profile's compiled line patterns.
pub(super) struct Matchers {
    pub declaration: Regex,
    pub assertion: Regex,
}

static MATCHERS: LazyLock<BTreeMap<&'static str, Matchers>> = LazyLock::new(|| {
    languages::all()
        .iter()
        .map(|profile| {
            let matchers = Matchers {
                declaration: Regex::new(profile.test_declaration)
                    .expect("language profile declaration regex must compile"),
                assertion: Regex::new(profile.assertion)
                    .expect("language profile assertion regex must compile"),
            };
            (profile.name, matchers)
        })
        .collect()
});

pub(super) fn matchers(profile: &LanguageProfile) -> &'static Matchers {
    &MATCHERS[profile.name]
}

/// A regular file in the base tree that a language profile covers.
pub(super) struct BaseTestFile {
    pub path: String,
    pub profile: &'static LanguageProfile,
}

/// Every regular file of `base`'s tree that `languages::for_path` maps to a
/// profile. Symlinks and submodules hold no test lines.
pub(super) fn base_test_files(worktree: &Path, base: &str) -> Result<Vec<BaseTestFile>> {
    let listing = git(worktree, &["ls-tree", "-r", "-z", "--full-tree", base])?;
    Ok(listing
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let (meta, path) = std::str::from_utf8(entry).ok()?.split_once('\t')?;
            let mode = meta.split(' ').next()?;
            if !matches!(mode, "100644" | "100755") {
                return None;
            }
            let profile = languages::for_path(path)?;
            Some(BaseTestFile {
                path: path.to_string(),
                profile,
            })
        })
        .collect())
}

#[derive(Debug, Default, Clone, Copy)]
struct Counts {
    declarations: u64,
    assertions: u64,
}

impl Counts {
    fn of(content: &[u8], profile: &LanguageProfile) -> Self {
        let matchers = matchers(profile);
        let mut counts = Self::default();
        for line in String::from_utf8_lossy(content).lines() {
            counts.declarations += u64::from(matchers.declaration.is_match(line));
            counts.assertions += u64::from(matchers.assertion.is_match(line));
        }
        counts
    }

    fn add(&mut self, other: Self) {
        self.declarations += other.declarations;
        self.assertions += other.assertions;
    }
}

#[derive(Debug, Default)]
struct Totals {
    base: Counts,
    current: Counts,
}

/// One event per language and measure whose current total is below its base
/// total. A base test file the fingerprint does not list is unchanged, so its
/// worktree content counts for both sides.
pub(super) fn total_events(
    worktree: &Path,
    changes: &ChangeFingerprint,
    base_files: &[BaseTestFile],
) -> Result<Vec<IntegrityEvent>> {
    let mut totals: BTreeMap<&'static str, Totals> = BTreeMap::new();
    for file in base_files {
        let changed = changes.files.contains_key(&file.path);
        let content = if changed {
            let object = format!("{}:{}", changes.base, file.path);
            git(worktree, &["cat-file", "blob", &object])?
        } else {
            read_current(worktree, &file.path)?
        };
        let counts = Counts::of(&content, file.profile);
        let language = totals.entry(file.profile.name).or_default();
        language.base.add(counts);
        if !changed {
            language.current.add(counts);
        }
    }
    let present = changes
        .files
        .keys()
        .filter(|path| current_sha256(changes, path).is_some());
    for path in present {
        if let Some(profile) = languages::for_path(path) {
            let counts = Counts::of(&read_current(worktree, path)?, profile);
            totals.entry(profile.name).or_default().current.add(counts);
        }
    }
    Ok(totals
        .into_iter()
        .flat_map(|(language, totals)| fallen(language, &totals))
        .collect())
}

fn read_current(worktree: &Path, path: &str) -> Result<Vec<u8>> {
    read_bounded(worktree, Path::new(path), MAX_TEST_FILE_BYTES)
        .with_context(|| format!("reading test file {path}"))
}

fn fallen(language: &str, totals: &Totals) -> Vec<IntegrityEvent> {
    let (base, current) = (totals.base, totals.current);
    [
        (
            EventKind::DeclTotal,
            "decl",
            base.declarations,
            current.declarations,
        ),
        (
            EventKind::AssertTotal,
            "assert",
            base.assertions,
            current.assertions,
        ),
    ]
    .into_iter()
    .filter(|(_, _, base, current)| current < base)
    .map(|(kind, measure, base, current)| IntegrityEvent {
        id: format!("TI-{measure}-{language}"),
        kind,
        language: Some(language.to_string()),
        path: None,
        base: Some(base),
        current: Some(current),
        current_sha256: None,
        detail: Vec::new(),
    })
    .collect()
}
