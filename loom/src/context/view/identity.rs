//! What a view was produced from and by.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::context::extract::lexical::LEXICAL_PARSER_VERSION;
use crate::context::extract::{registry, BoxedExtractor};
use crate::context::source_graph::{body_hash, GRAPH_SCHEMA_VERSION};
use crate::context::store::canonical_json;

/// Version of the cross-file resolution rules. Any change to rules 1-7 bumps
/// it, so a view resolved under the old rules is never served or relinked.
pub const RESOLVER_VERSION: u32 = 4;

/// The inputs a view is a pure function of: the revision and overlay it
/// describes, and the graph schema, extractors and resolver that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewIdentity {
    /// [`GRAPH_SCHEMA_VERSION`] the view was built under.
    pub schema_version: u32,
    /// Revision of the base layer underneath.
    pub base_revision: String,
    /// Generation of the overlay applied, empty for a base view.
    pub overlay_generation: String,
    /// Digest of the registered extractors' dialects and parser versions.
    pub extractor_digest: String,
    /// [`RESOLVER_VERSION`] the view was resolved under.
    pub resolver_version: u32,
}

impl ViewIdentity {
    /// The identity this build gives a view of `base_revision` with the
    /// overlay generation `overlay_generation`.
    pub fn current(base_revision: &str, overlay_generation: &str) -> Self {
        Self {
            schema_version: GRAPH_SCHEMA_VERSION,
            base_revision: base_revision.to_string(),
            overlay_generation: overlay_generation.to_string(),
            extractor_digest: extractor_digest(&registry()),
            resolver_version: RESOLVER_VERSION,
        }
    }

    /// The first 12 hex digits of the `sha256` over this identity's canonical
    /// JSON, for file names.
    pub fn digest12(&self) -> String {
        let json = canonical_json(self).expect("an identity of strings and integers serializes");
        let digest = body_hash(json.as_bytes());
        let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
        hex.chars().take(12).collect()
    }

    /// Whether a view built under `previous` can seed a relink to this
    /// identity: same schema, extractors and resolver. Equal bytes under
    /// another extractor can extract differently, so an unchanged file's
    /// previous entry is only reusable when all three match.
    pub(crate) fn relinkable_from(&self, previous: &ViewIdentity) -> bool {
        self.schema_version == previous.schema_version
            && self.extractor_digest == previous.extractor_digest
            && self.resolver_version == previous.resolver_version
    }

    /// The parser version each of `extractors` stamps, by dialect id.
    pub fn extractor_versions(extractors: &[BoxedExtractor]) -> BTreeMap<String, String> {
        extractors
            .iter()
            .map(|extractor| {
                (
                    extractor.dialect().id.to_string(),
                    extractor.cache_identity().to_parser_version(),
                )
            })
            .collect()
    }
}

/// `sha256:<hex>` over the sorted `"{dialect}={parser_version}"` lines of
/// `extractors` plus a `lexical=<LEXICAL_PARSER_VERSION>` line: the lexical
/// fallback stamps gap, unknown-type and oversized entries.
pub(crate) fn extractor_digest(extractors: &[BoxedExtractor]) -> String {
    digest_versions(extractors, LEXICAL_PARSER_VERSION)
}

/// [`extractor_digest`] with `lexical_version` as the lexical fallback's
/// parser version.
pub(super) fn digest_versions(extractors: &[BoxedExtractor], lexical_version: &str) -> String {
    let mut lines: Vec<String> = ViewIdentity::extractor_versions(extractors)
        .into_iter()
        .map(|(dialect, parser_version)| format!("{dialect}={parser_version}\n"))
        .collect();
    lines.push(format!("lexical={lexical_version}\n"));
    lines.sort();
    body_hash(lines.concat().as_bytes())
}
