# W1: summary format and store layout

Tier: opus (`loom-senior-software-engineer`). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md`
first ("Format rules" is your specification).

## Files you own

`loom/src/planning/summary/mod.rs`, `format.rs`, `format/body.rs`, `format/tests.rs` (all under
`loom/src/planning/summary/`), and `loom/src/planning/mod.rs` (the seed added `pub mod summary;`; add
nothing else). Tier: sonnet. The sections "store.rs" and "format/front.rs" below are the rules codex
units C1 (`store.rs`, `store/tests.rs`) and C2 (`format/front.rs`, `format/front_tests.rs`) implement in
your wave; build against their pinned signatures (`c1-store.md`, `c2-front.md`), define
`pub struct FormatError { pub line: usize, pub message: String }` in format.rs exactly so, and
re-export `pub use front::{FrontMatter, SummaryKind};` from format.rs.

## `store.rs`

Common.md "Store and scope". Rules from section 04 "The store":

- `summary_id_for(project, rel)`: a `ProductSection { number }` document (listed or unlisted) ->
  `product/section_<nn>`; a plan whose winning path is directly in `doc/plans/` (any state) ->
  `plans/<slug>`; a plan under `doc/plans/archive/` -> `plans/<slug>` only when some metaplan entry
  names its slug; everything else `None`. Strays never get a summary of their own.
- `scope(project)`: `product` = the sections the vision's table lists, in table order, that exist;
  `engineering` = the plans metaplan entries name, in metaplan then heading order, that exist;
  `outside` = unlisted sections and plans with a summary but no metaplan entry.
- `expected_ids(project)`: every document summary id, then `product` when the vision exists and
  `engineering` when a metaplan exists.
- `validate_id(id) -> anyhow::Result<()>` per common.md "Rules for every worker", and
  `summary_path(id)`; `source_of(project, id) -> Option<PathBuf>` (the current source path).
- Relative link helpers: `link_from_summary(summary_id, target_rel: &Path) -> String` and its
  inverse `resolve_link(summary_id, href) -> Option<(PathBuf, Option<String>)>` (lexical, no
  filesystem; `..` beyond the repository root -> `None`).

## `format/body.rs`

The line classifier and sentence extraction of "Format rules" 1-6: produce `Vec<Block>` with
`Sentence { id, text, links, line }` (0-based `line` in the type, 1-based in `FormatError`).
Link parsing: a `[label](target)` with balanced brackets; targets with spaces or `<...>` forms are
rejected. `sentence_id` lives here and is re-exported from `format`.

## `format/front.rs`

`FrontMatter` parse with `serde_saphyr::from_str` into a private serde struct (`deny_unknown_fields`)
and the deterministic emitter (common.md "Front matter"). Keys must be quoted when they contain `#`
or `:` followed by a space; the emitter always double-quotes keys and values in `cites`/`members`
and escapes `"` and `\` (write a round-trip test: emit -> parse -> equal).

## `format.rs`

`parse(summary_rel, text)`: split front matter (`---` first line through the next `---` line), parse
both parts, resolve every link with `store::resolve_link` into `LinkTarget::Source` (a path outside
the store) or `LinkTarget::Sentence` (a path inside the store with an `s-` fragment) or
`Unresolved`. Conflict markers -> error at that line. `parse_draft(id, text)` for drafts (no front
matter allowed). `render(&SummaryFile) -> String` and `render_body` used by seal (W3) and the engine
(W2). Validation of link TARGETS against sources (rules 7, 8) is W3's seal and W2's evaluator; your
parse only classifies.

## Tests (yours)

`format/tests.rs`: every accepted and rejected line kind with its line number; a heading with a
link; trailing links with several labels; the sentence-id example from the contract
(`The app keeps no state.`); front matter round trip with `#`, `:` and `"` in keys; conflict
markers; `render` stable across two calls. `store/tests.rs`: `summary_id_for` for every row of
section 04's table (listed and unlisted sections, every plan state, archived named and unnamed),
scope order, `validate_id` cases, `resolve_link` for `../../../product/section_01.md#a` from
`product/section_01` and for `product/section_01.md#s-1234abcd` from `product`.

## Proof

`cargo test --lib planning::summary::`, `cargo clippy --all-targets -- -D warnings`. Report every
public item.
