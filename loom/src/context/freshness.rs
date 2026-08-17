//! How current a derived retrieval layer is.
//!
//! Split out of [`crate::context::schema`] so the shared type contract stays
//! readable; [`crate::context::schema`] re-exports [`Freshness`], so every
//! existing import path still resolves. The values themselves are produced by
//! [`crate::context::refresh`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How current a derived layer is relative to the bytes it was built from.
///
/// A pack carries two: `structural` (the chunk catalog vs the markdown on disk)
/// and `semantic` (the source graph vs the tracked source tree). They move
/// independently — editing a `.rs` file staleness the semantic layer while the
/// knowledge catalog stays perfectly fresh.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freshness {
    /// Content revision the layer was built from (hex sha256, or empty if never built).
    #[serde(default)]
    pub revision: String,
    /// When the layer was last rebuilt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<DateTime<Utc>>,
    /// True when the on-disk inputs no longer match `revision`.
    #[serde(default)]
    pub stale: bool,
    /// Human-readable cause when `stale` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Freshness {
    /// A layer that has never been built. Reported as stale so callers never
    /// mistake "no data" for "up to date".
    pub fn never_built(detail: impl Into<String>) -> Self {
        Freshness {
            revision: String::new(),
            computed_at: None,
            stale: true,
            detail: Some(detail.into()),
        }
    }
}
