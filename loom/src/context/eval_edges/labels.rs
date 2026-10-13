//! The `labels.yaml` of a labelled corpus: what a compiler would bind, written
//! before the extractor ran.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::context::extract::dialect::dialect_by_id;

/// File name of the labels at a corpus root.
pub const LABELS_FILE: &str = "labels.yaml";

/// Suffix a syntax-error file carries on disk, so editors and compilers leave
/// it alone. The corpus loads it under the path without the suffix.
pub const STORED_SUFFIX: &str = ".txt";

/// One corpus's labels.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub dialect: String,
    #[serde(default)]
    pub declarations: Vec<DeclarationLabel>,
    #[serde(default)]
    pub references: Vec<ReferenceLabel>,
    #[serde(default)]
    pub impact: Vec<ImpactLabel>,
    /// Corpus-relative paths of files that do not parse, each stored on disk
    /// as `<path>.txt`.
    #[serde(default)]
    pub syntax_error_files: Vec<String>,
}

/// A declaration the extractor must find: matched on `(path, kind, scope)`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationLabel {
    pub path: String,
    pub kind: String,
    pub scope: Vec<String>,
    pub line: usize,
}

/// A reference site and what it must resolve to.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLabel {
    pub path: String,
    pub line: usize,
    pub symbol: String,
    pub expect: Expect,
}

/// The raw `expect` mapping: exactly one field is set.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    target: Option<String>,
    external: Option<bool>,
    ambiguous: Option<Vec<String>>,
}

/// What a reference label expects of its edge.
#[derive(Debug, PartialEq)]
pub enum Expectation<'a> {
    /// The edge binds to this node id.
    Target(&'a str),
    /// The target lies outside the corpus: the edge stays unbound.
    External,
    /// The edge stays unbound and lists exactly these candidate ids.
    Ambiguous(&'a [String]),
}

impl Expect {
    /// The single expectation this mapping states.
    pub fn expectation(&self) -> Expectation<'_> {
        match (&self.target, self.external, &self.ambiguous) {
            (Some(target), _, _) => Expectation::Target(target),
            (_, Some(_), _) => Expectation::External,
            (_, _, Some(candidates)) => Expectation::Ambiguous(candidates),
            // `validate` refuses a label with no field set.
            (None, None, None) => Expectation::External,
        }
    }

    fn validate(&self) -> Result<()> {
        let set = usize::from(self.target.is_some())
            + usize::from(self.external.is_some())
            + usize::from(self.ambiguous.is_some());
        if set != 1 {
            bail!("expect needs exactly one of target, external, ambiguous (found {set})");
        }
        if self.external == Some(false) {
            bail!("expect.external must be true when present");
        }
        Ok(())
    }
}

/// A reverse-impact expectation: every id in `expect` is reached from `start`
/// within `depth` hops.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactLabel {
    pub start: String,
    pub depth: usize,
    pub expect: Vec<String>,
}

/// Read and validate `<dir>/labels.yaml`.
pub fn load_labels(dir: &Path) -> Result<Labels> {
    let path = dir.join(LABELS_FILE);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let labels: Labels =
        crate::yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    if dialect_by_id(&labels.dialect).is_none() {
        bail!("{}: unknown dialect '{}'", path.display(), labels.dialect);
    }
    for label in &labels.references {
        label.expect.validate().with_context(|| {
            format!(
                "{}: reference {}:{} {}",
                path.display(),
                label.path,
                label.line,
                label.symbol
            )
        })?;
    }
    Ok(labels)
}
