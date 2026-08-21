//! Where a channel's lexical index lives on disk, and the best-effort IO
//! around it.
//!
//! ## Nothing here may fail a retrieval
//!
//! Every read and every write is best-effort and silent. The prompt hook runs
//! against checkouts that are read-only, sandboxed, or on a full disk, and a
//! context pack is still the right answer in all three: a cache that cannot be
//! written costs latency, while a retrieval that returns `Err` because a cache
//! directory was unwritable costs the user their context. So [`LexicalCache`]
//! returns `Option`/`()` rather than `Result`, and reports the reason at
//! `tracing::debug!`.
//!
//! ## Keys
//!
//! A file name embeds the revision of the corpus it describes, so a stale index
//! is never *read* — it simply is not the file the reader asks for — and the
//! staleness question reduces to garbage collection, which
//! [`LexicalCache::save`] does by bounding each channel to [`KEEP_INDEXES`]
//! files rather than to the one just written. See that constant for why the
//! difference matters to parallel stages.

use super::LexicalIndex;
use crate::context::graph_store::ResolvedGraph;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Directory holding the per-revision index files, relative to the context
/// cache root ([`crate::context::store::CACHE_RELATIVE_DIR`]).
pub const LEXICAL_RELATIVE_DIR: &str = "lexical";

/// Longest revision accepted in a file name. Long enough for a full sha256 hex
/// digest with room to spare, short enough that no path built from it can hit a
/// filesystem name limit.
const REVISION_NAME_MAX: usize = 80;

/// Index files retained per channel, newest first.
///
/// Not one. The source key is derived from the RESOLVED graph, and every
/// parallel stage resolves a different overlay, so N concurrent worktrees mean
/// N live keys against one shared cache directory. Keeping only the file just
/// written would have each stage unlink every sibling stage's index on every
/// prompt: permanent misses, a multi-megabyte write per prompt, and a cache
/// that is strictly worse than no cache — inside a hook with a five-second
/// ceiling. Six covers a normal fan-out plus the host's own working-tree
/// overlay; the files are rebuildable and live under `.loom/cache/`.
const KEEP_INDEXES: usize = 6;

/// Which channel's corpus an index file describes.
///
/// The two channels have separate files because they have separate corpora,
/// separate revisions, and separate lifetimes: the knowledge catalog changes
/// when someone edits prose, the source layer when code is committed or a stage
/// overlay is republished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexChannel {
    /// Curated knowledge chunks, keyed by the catalog revision.
    Knowledge,
    /// Source-graph nodes, keyed by the resolved layer ([`source_layer_key`]).
    Source,
}

impl IndexChannel {
    /// File-name prefix. Also the prefix [`LexicalCache::save`] sweeps for, so
    /// the two channels must not prefix one another.
    fn prefix(self) -> &'static str {
        match self {
            Self::Knowledge => "knowledge",
            Self::Source => "source",
        }
    }
}

/// A handle on one channel's index file, for one corpus revision.
///
/// Construct it from the context cache root — the same root
/// [`crate::context::store::ContextStore::root`] resolves, which follows a
/// worktree's `.work` symlink to the main project, so parallel stages share one
/// index rather than each building its own.
#[derive(Debug, Clone)]
pub struct LexicalCache {
    directory: PathBuf,
    prefix: &'static str,
    /// The revision as the caller spelled it. This is what goes INSIDE the file
    /// and what `accepts` compares — never the sanitized form, so two revisions
    /// that sanitize alike (or share an 80-character prefix) still fail the
    /// check rather than silently passing it.
    revision: String,
    /// The revision folded into a file name.
    file_stem: String,
}

impl LexicalCache {
    /// A cache handle for `channel` at `revision` under `cache_root`.
    pub fn new(cache_root: &Path, channel: IndexChannel, revision: &str) -> Self {
        Self {
            directory: cache_root.join(LEXICAL_RELATIVE_DIR),
            prefix: channel.prefix(),
            revision: revision.to_string(),
            file_stem: sanitize_revision(revision),
        }
    }

    /// The knowledge channel's cache, keyed by the catalog revision.
    pub fn knowledge(cache_root: &Path, catalog_revision: &str) -> Self {
        Self::new(cache_root, IndexChannel::Knowledge, catalog_revision)
    }

    /// The source channel's cache, keyed by the resolved graph it will index.
    pub fn source(cache_root: &Path, graph: &ResolvedGraph) -> Self {
        Self::new(cache_root, IndexChannel::Source, &source_layer_key(graph))
    }

    /// The revision this handle reads and writes, exactly as it was given.
    ///
    /// This is the string that goes inside the file and that `accepts`
    /// compares — never the folded file name, so two revisions that fold alike
    /// are still told apart.
    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    /// The index for this revision, or `None` on any doubt whatsoever.
    ///
    /// `doc_ids` are the current corpus's document identities, in corpus order;
    /// a file that does not describe exactly those documents is a miss.
    pub(crate) fn load(&self, doc_ids: &[&str]) -> Option<LexicalIndex> {
        match self.read(doc_ids) {
            Ok(index) => Some(index),
            Err(reason) => {
                tracing::debug!(
                    path = %self.path().display(),
                    %reason,
                    "lexical index miss; falling back to the corpus scan"
                );
                None
            }
        }
    }

    /// Persist `index`, then bound this channel's file count.
    ///
    /// Pruning happens ONLY after a successful write. Unlinking first, or
    /// unlinking regardless, would turn "this checkout cannot write the cache"
    /// into "this checkout deletes the cache every prompt and then rebuilds
    /// nothing" — strictly worse than leaving a stale sibling on disk, which
    /// costs nothing because a stale sibling is never the file a reader asks
    /// for.
    pub(crate) fn save(&self, index: &LexicalIndex) {
        let path = self.path();
        match self.write(&path, index) {
            Ok(()) => self.prune_siblings(&path),
            Err(error) => tracing::debug!(
                path = %path.display(),
                %error,
                "lexical index not written; retrieval is unaffected"
            ),
        }
    }

    /// Path of this handle's file.
    fn path(&self) -> PathBuf {
        self.directory
            .join(format!("{}-{}.json", self.prefix, self.file_stem))
    }

    /// Read and validate, reporting why not.
    fn read(&self, doc_ids: &[&str]) -> Result<LexicalIndex, String> {
        let content = fs::read_to_string(self.path()).map_err(|error| error.to_string())?;
        let index: LexicalIndex =
            serde_json::from_str(&content).map_err(|error| error.to_string())?;
        index
            .accepts(&self.revision, doc_ids)
            .map_err(str::to_string)?;
        Ok(index)
    }

    /// Serialize and write under the directory lock.
    ///
    /// Compact JSON, deliberately not [`crate::context::store::canonical_json`]:
    /// this file holds one entry per (term, document) pair — well over a hundred
    /// thousand of them on a real repository — and pretty-printing puts every
    /// posting's two integers on separate lines, inflating a 4 MB machine-read
    /// artifact by roughly an order of magnitude for a reader that does not
    /// exist. Determinism does not come from the formatting: it comes from
    /// `BTreeMap` ordering and from `Vec`s in corpus order, which compact output
    /// preserves exactly.
    ///
    /// The directory is created by `locked_write`'s own `lock_parent_dir`, so
    /// there is no `create_dir_all` here to disagree with it later.
    fn write(&self, path: &Path, index: &LexicalIndex) -> Result<()> {
        let content = serde_json::to_string(index)?;
        crate::fs::locking::locked_write(path, &content)
    }

    /// Keep the [`KEEP_INDEXES`] newest of this channel's files, `keep`
    /// included, and unlink the rest.
    fn prune_siblings(&self, keep: &Path) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        let prefix = format!("{}-", self.prefix);
        let mut siblings: Vec<(SystemTime, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if path == keep || !name.starts_with(&prefix) || !name.ends_with(".json") {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            siblings.push((modified, path));
        }

        // Newest first, and by path on a tie: two writes inside one filesystem
        // timestamp tick must still evict the same file on every machine.
        siblings.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        for (_, path) in siblings.into_iter().skip(KEEP_INDEXES - 1) {
            if let Err(error) = fs::remove_file(&path) {
                tracing::debug!(path = %path.display(), %error, "stale lexical index left behind");
            }
        }
    }
}

/// Identity of the resolved source layer a source index describes.
///
/// The proposal specified `sha256(base_revision + ":" + overlay_fingerprint)`
/// over the overlay FILE's content hash. This hashes the resolved view instead —
/// the base revision plus every resolved file's path and content hash — for two
/// reasons. It is what is actually indexed: `GraphStore::resolved` shadows base
/// entries with overlay entries per path, so two different overlays that resolve
/// to the same files must share an index, and one overlay whose bytes changed
/// without its path set changing must NOT. And it is derivable from the
/// [`ResolvedGraph`] the ranker already holds, so the key cannot drift from the
/// corpus it names — there is no second code path computing it from other
/// inputs.
///
/// The extractor identity is mixed in for a case the content hashes cannot
/// see: a parser upgrade re-derives different `scope` and `signature` text from
/// byte-identical files, and a node id (`<path>#<kind>:<scope>`) does not always
/// change with it. Without this, an index built by the old extractor would keep
/// serving its old tokens against the new graph. One node per file is enough to
/// carry it, because extraction is per FILE: every node in a
/// [`crate::context::graph_store::FileEntry`] was
/// produced by one pass of one parser, so they all share a `parser_version` and
/// the first is a faithful representative of the rest.
///
/// ## Order, and why it is not an accident
///
/// [`ResolvedGraph::files`] is a `BTreeMap`, so this iterates in sorted path
/// order on every machine and every run. That is load-bearing, not incidental:
/// a key that varied with iteration order would hash differently each time and
/// miss the cache FOREVER, silently, while every test that only checks
/// correctness still passed. Do not change this container to a `HashMap`, and
/// do not "optimize" the loop into anything that does not preserve its order.
///
/// ## Cost
///
/// This runs once per query and hashes O(files) short strings — a path, a
/// content hash and a parser version per resolved file, on the order of 100 KB
/// of sha256 input for this repository. What it buys is skipping O(documents ×
/// terms) tokenization: `tokenize` over the scope and signature of every one of
/// ~7,900 source nodes, scanning per character, lowercasing, and allocating a
/// `String` per emitted token. The two are not close — the key is microseconds
/// against tens of milliseconds — and the key is paid on hits AND misses while
/// the tokenization is what the hit avoids entirely.
///
/// Truncated to 16 hex characters: this is a cache-collision domain, not a
/// security one, and 64 bits over a per-repository population of a few thousand
/// revisions is far past the point where a collision is the least likely thing
/// that could go wrong. `doc_ids` validation on load catches one anyway.
pub fn source_layer_key(graph: &ResolvedGraph) -> String {
    let mut hasher = Sha256::new();
    hasher.update(graph.base_revision.as_bytes());
    hasher.update(b":");
    for (path, entry) in &graph.files {
        hasher.update(path.as_bytes());
        hasher.update(b"=");
        hasher.update(entry.content_hash.as_bytes());
        hasher.update(b"@");
        if let Some(node) = entry.nodes.first() {
            hasher.update(node.parser_version.as_bytes());
        }
        hasher.update(b"\n");
    }
    hex::encode(&hasher.finalize()[..8])
}

/// Fold a revision into something safe to put in a file name.
///
/// Every production caller passes a hex digest, so this normally changes
/// nothing. It exists because the string reaches `Path::join`: a revision
/// containing `../` would place the file outside the cache directory, and a
/// revision containing a path separator would place it in a directory that does
/// not exist. The guard belongs here rather than in each caller's good manners.
fn sanitize_revision(revision: &str) -> String {
    revision
        .chars()
        .take(REVISION_NAME_MAX)
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}
