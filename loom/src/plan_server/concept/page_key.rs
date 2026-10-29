//! Feature-qualified concept page keys.

use crate::planning::kinds::is_feature_name;

/// Which overview a page belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Product sections of a feature.
    Product,
    /// Engineering plans of a feature.
    Engineering,
}

/// The page a request addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageKey {
    /// An overview of one feature.
    Overview {
        /// The owning feature.
        feature: String,
        /// The overview's document family.
        scope: Scope,
    },
    /// A section or plan summary; standalone plans have no feature.
    Summary {
        /// The owning feature, absent for a standalone plan.
        feature: Option<String>,
        /// The document family.
        scope: Scope,
        /// A product section stem or plan slug.
        doc: String,
    },
}

impl PageKey {
    /// Parse the path after `/api/concept/`, validating feature and document names.
    pub fn parse(rest: &str) -> Option<Self> {
        let parts: Vec<_> = rest.split('/').collect();
        match parts.as_slice() {
            ["overview", feature, scope] if is_feature_name(feature) => Some(Self::Overview {
                feature: (*feature).to_owned(),
                scope: parse_scope(scope)?,
            }),
            [feature, scope, doc] if is_feature_name(feature) && safe_doc(doc) => {
                Some(Self::Summary {
                    feature: Some((*feature).to_owned()),
                    scope: parse_scope(scope)?,
                    doc: (*doc).to_owned(),
                })
            }
            ["plans", doc] if safe_doc(doc) => Some(Self::Summary {
                feature: None,
                scope: Scope::Engineering,
                doc: (*doc).to_owned(),
            }),
            _ => None,
        }
    }

    /// The key as it appears in URLs and `DocLink.page`.
    pub fn as_key(&self) -> String {
        match self {
            Self::Overview { feature, scope } => {
                format!("overview/{feature}/{}", scope_name(*scope))
            }
            Self::Summary {
                feature: Some(feature),
                scope,
                doc,
            } => {
                format!("{feature}/{}/{doc}", scope_name(*scope))
            }
            Self::Summary {
                feature: None, doc, ..
            } => format!("plans/{doc}"),
        }
    }

    /// The owning feature, absent for a standalone plan.
    pub fn feature(&self) -> Option<&str> {
        match self {
            Self::Overview { feature, .. } => Some(feature),
            Self::Summary { feature, .. } => feature.as_deref(),
        }
    }
}

fn parse_scope(scope: &str) -> Option<Scope> {
    match scope {
        "product" => Some(Scope::Product),
        "engineering" => Some(Scope::Engineering),
        _ => None,
    }
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Product => "product",
        Scope::Engineering => "engineering",
    }
}

fn safe_doc(doc: &str) -> bool {
    (1..=128).contains(&doc.len())
        && doc
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && doc != "."
        && doc != ".."
}
