# Plan `plan-summary-store`: common brief

Read this first, then your own brief. This file is authoritative for the public API of
`loom::planning::summary` (plans 5, 8 and 10 are written against it) and for the summary format.
Change a name or signature only with a `loom memory decision` naming the change and why, reported
to the orchestrator.

## The spec you implement

`doc/product/section_04.md` in full except "Who rebuilds what" (only its "Models" paragraph is
yours), "Asking before a large rebuild" and "Cost" (plan 10). Also section 12 "Paths the server
reads that sessions can write" and "Symlinks under `doc/`", and section 02 "Runtime state".

## What plan 1 gives you (merged)

`loom::planning::{discover, sections, paths, runtime, watch, project}`; read
`doc/plans/briefs/plan-documents/common.md` "Public API" for the exact types. You use:
`Project::load(root)`, `Project::docs[path].sections` (`DocSections::find(anchor)`, `file_hash`),
`Project::vision_rows`, `Project::metaplans`, `Project::tree.plans`, `sections::normalized_hash`,
`runtime::{runtime_paths, runtime_paths_under, RuntimePaths}`, `watch::{Batch, OwnWrites}`,
`discover::read_doc`.

## Format rules (decided)

Paths in a summary are relative to the summary file. Repo-relative paths in front matter
(`of`, `cites` keys) start with `doc/`. The draft is the BODY only (no front matter); `seal`
refuses a draft that starts with `---`.

Body, line by line (1-based line numbers in every message, counted in the draft):

1. The first non-blank line is `# <title>` (no links). Required.
2. Allowed afterwards: blank lines, `# ` or `## ` headings with no link, and sentence lines.
3. Rejected lines: a list item (`- `, `* `, `+ `, `<digits>. ` or `<digits>) ` at column 0), a table
   row (starts with `|`), a fence (```` ``` ```` or `~~~`), an indented line (4 spaces or a tab
   first), a block quote (`>`), HTML (`<` followed by a letter, `/` or `!` anywhere outside a link
   target), a `###`+ heading, a heading with a link.
4. A sentence line: text, then one or more Markdown links `[label](target)` separated by spaces at
   the END of the line. The text (links removed, whitespace collapsed, trimmed) is non-empty and
   holds no other link. No trailing link -> rejected ("sentence has no citation link").
5. Duplicate sentence ids in one summary -> rejected, naming both lines.
6. More than 15 sentences (document) or 30 (overview) -> rejected.
7. Links, document summary `product/section_<nn>` or `plans/<slug>`: every target resolves to the
   summary's own source file (its current path); the fragment, when present, is an anchor
   `DocSections::find` accepts (headings, `loom-metadata`, `stage:<id>`); no fragment = preamble.
8. Links, overview `product`: a member summary sentence (`product/section_<nn>.md#s-<8 hex>` relative
   to the store root) whose summary exists, is not missing and whose sentence exists and is current;
   or a heading section of the vision. Overview `engineering`: member `plans/<slug>.md#s-...`
   sentences, or heading sections of a metaplan. Every member summary in scope is cited at least
   once, and so is the vision (product) or every metaplan (engineering); an uncited one is rejected
   by id.

Sentence id: `s-` + the first 8 hex digits of SHA-256 of the sentence text (rule 4's text),
computed with `context::source_graph::body_hash` and the `sha256:` prefix stripped.

Front matter, rendered in exactly this order, maps sorted by key, values double-quoted only when
YAML needs it (render with a hand-written emitter; parse with `serde-saphyr`):

```text
---
kind: document | overview
of: <source path> | product | engineering
file: <sha256:...>            (document only)
cites:
  <path>#<anchor>: <sha256:...>   (key "<path>" with no "#" for a preamble cite)
members:                      (overview only)
  <summary id>: <sha256:...>
---
```

then one blank line, then the body exactly as drafted with trailing whitespace removed per line and
one final newline. `members` values: `normalized_hash` of the member's ordered sentence ids joined
by `\n`. `cites` holds every raw passage the body links (overviews: vision or metaplan passages).

A summary file is **missing** when absent, when its front matter or body fails these rules (a hand
edit), or when any line starts with `<<<<<<< `, `=======` or `>>>>>>> `.

## Rules for every worker

- Files at most 400 lines, functions at most 50 (`cargo test --test maintainability`; the exact
  ledger `loom/maintainability-baseline.txt` gets no new row). Split into submodules early.
- Every read of a session-writable path (`doc/`, `drafts/`, `claims/`) goes through
  `crate::fs::safe_read`; every write under `doc/loom/summaries/` through `crate::fs::safe_fs`
  (`safe_open_dirfd` `safe_fs.rs:59`, `safe_create_dir_all` `:746`, `safe_create_new_in_workdir`
  `:375`, `safe_rename_in_workdir` `:413`, `safe_remove_in_workdir` `:649`) inside `with_store_lock`.
  Never `std::fs::write`/`rename`/`File::create` on those paths.
- Summary ids are validated before any path is built: `product`, `engineering`,
  `product/section_<two or more digits>`, `plans/<slug>` with `<slug>` matching
  `[A-Za-z0-9][A-Za-z0-9._-]*` and not containing `..`. Anything else is an error naming the id.
- Library code returns `anyhow::Result`; no `unwrap`/`expect` outside tests; every public item has a
  `///` doc comment (`cargo doc -D warnings`).
- No new dependency (`serde-saphyr`, `serde`, `serde_json`, `fs2`, `chrono` are in `loom/Cargo.toml`).
- Record mistakes, decisions and surprises with `loom memory` at once; never `loom knowledge`;
  never Claude Code auto-memory; no state-changing git commands.

## Public API (authoritative)

Module `loom::planning::summary` (`loom/src/planning/summary/`).

**Store and scope** [`summary-store`] `summary::store`:
- `pub const STORE_DIR: &str = "doc/loom/summaries"`; `pub fn summary_path(id: &str) -> PathBuf` (`<STORE_DIR>/<id>.md`);
  `pub fn summary_id_for(project: &Project, rel: &Path) -> Option<String>` (`product/section_<nn>`, `plans/<slug>`;
  archived plans only when a metaplan names them).
- `pub struct Scope { pub product: Vec<String>, pub engineering: Vec<String>, pub outside: Vec<String> }`;
  `pub fn scope(project: &Project) -> Scope` (overview members, in reading order; `outside` = unlisted sections, plans no metaplan names).
- `pub fn expected_ids(project: &Project) -> Vec<String>` (every document summary plus `product`, `engineering` when their scope is non-empty).

**File format** [`summary-store`] `summary::format`:
- `pub enum SummaryKind { Document, Overview }`
- `pub struct FrontMatter { pub kind: SummaryKind, pub of: String, pub file: Option<String>, pub cites: BTreeMap<String /* path#anchor */, String /* sha256:.. */>, pub members: BTreeMap<String /* summary id */, String> }`
- `pub struct Link { pub label: String, pub target: String /* as written */, pub resolved: LinkTarget }`,
  `pub enum LinkTarget { Source { path: PathBuf, anchor: String }, Sentence { summary: String, id: String }, Unresolved(String) }`
- `pub struct Sentence { pub id: String, pub text: String /* links removed, whitespace collapsed */, pub links: Vec<Link>, pub line: usize }`
- `pub enum Block { Heading { level: u8, text: String, line: usize }, Paragraph(Vec<Sentence>) }`
- `pub struct SummaryFile { pub front: FrontMatter, pub title: String, pub blocks: Vec<Block> }`
- `pub fn sentence_id(text_without_links: &str) -> String` (`s-` + first 8 hex of sha256 of whitespace-collapsed text)
- `pub fn parse(summary_rel: &Path, text: &str) -> Result<SummaryFile, Vec<FormatError>>`; `pub struct FormatError { pub line: usize, pub message: String }`
- `pub fn render(file: &SummaryFile) -> String` (deterministic: keys sorted, no time or model).
- Limits `MAX_DOCUMENT_SENTENCES = 15`, `MAX_OVERVIEW_SENTENCES = 30`.

**Evaluation** [`summary-store`] `summary::evaluate`:
- `pub enum SentenceState { Current, Stale }`; `pub enum SummaryState { Current, Incomplete, Stale, Rebuilding, Missing }`
- `pub struct CitationEval { pub target: LinkTarget, pub current: bool }`
- `pub struct SentenceEval { pub id: String, pub text: String, pub line: usize, pub state: SentenceState, pub citations: Vec<CitationEval> }`
- `pub struct SummaryEval { pub id: String, pub source: Option<PathBuf> /* document */, pub state: SummaryState, pub file: Option<SummaryFile>, pub sentences: Vec<SentenceEval>, pub claim: Option<Claim>, pub error: Option<String> /* why missing */ }`
- `pub struct Evaluation { pub summaries: BTreeMap<String, SummaryEval>, pub scope: Scope, pub orphans: Vec<String> /* store files whose id is not expected */ }`
- `pub fn evaluate(project: &Project, claims: &[Claim]) -> Evaluation` (pure over the project plus summary files read no-follow).

**Engine (server-side driver)** [`summary-store`] `summary::engine`:
- `pub struct SummaryEngine`; `impl SummaryEngine { pub fn new(root: PathBuf, paths: RuntimePaths, own: OwnWrites) -> Self; pub fn on_batch(&mut self, project: &Project, batch: &Batch) -> Result<EngineUpdate> }`
- `pub struct EngineUpdate { pub evaluation: Evaluation, pub rewritten: Vec<String> /* same-hash renames applied */, pub deleted: Vec<String> /* source removed */, pub invalidated: Vec<String> /* ids that became stale, incomplete or missing in THIS batch */ }`
  Renames rewrite `of`, `cites` keys and body links through the store lock and register the new hash with `OwnWrites`.
  Plan 5 calls `on_batch` from the server's watcher callback and publishes `evaluation`; plan 10 reads `invalidated`.

**Locks, claims, drafts** [`summary-store`] `summary::{lock, claims}`:
- `pub fn with_store_lock<T>(paths: &RuntimePaths, f: impl FnOnce() -> Result<T>) -> Result<T>` (fs2 exclusive on `summaries.lock`, as `quota/shared.rs` `poll_shared`).
- `pub struct Claim { pub summary: String, pub session: Option<String> /* $LOOM_PLAN_SESSION */, pub pid: u32, pub created_at: String /* RFC 3339 */ }`
  stored as `claims/<id with '/' -> "__">.json`, created with `O_EXCL` (that IS the refusal test).
- `pub fn list_claims(paths: &RuntimePaths) -> Result<Vec<Claim>>`; `pub fn release(paths: &RuntimePaths, ids: &[String]) -> Result<Vec<String>>`;
  `pub fn clear_session_claims(paths: &RuntimePaths, session: &str) -> Result<Vec<String>>` (plan 10's server calls it);
  `pub fn draft_path(paths: &RuntimePaths, id: &str) -> PathBuf` (`drafts/<id>.md`).

**Commands** [`summary-store`] `summary::{seal, brief, status}`, CLI `PlanCommands::Summary { command: SummaryCommands }`:
- `SummaryCommands::Seal { id: String, #[arg(long)] force: bool }`, `Brief { ids: Vec<String>, #[arg(long)] note: Option<String> }`, `Status { #[arg(long)] json: bool }`, `Release { ids: Vec<String> }`.
- `pub fn seal(root: &Path, paths: &RuntimePaths, id: &str, force: bool) -> Result<SealOutcome, SealError>` (refuses an id claimed by another session; from a terminal any claimed id unless `force`); `SealError::Rejected(Vec<FormatError>)` prints `<draft path>:<line>: <message>` per problem, exit 1.
- `pub fn brief(root: &Path, paths: &RuntimePaths, ids: &[String], note: Option<&str>) -> Result<String>` (claims, then prints the brief).
- `loom plan summary status --json`: `{"schema":1,"summaries":[{"id","state","source","path","sentences":[{"id","line","state","text"}],"claim":{"session","pid","created_at"}|null,"error"}]}` (pinned by `loom/tests/fixtures/planning/summary_status_schema.json`).
- Inside a session the CLI reads only `drafts/`, `claims/`, `summaries.lock` from the runtime dir and needs no server.

**Config** [`summary-store`] `[plan]` keys `plan.document_summary_model` (haiku), `plan.document_summary_effort` (medium),
`plan.overview_model` (sonnet), `plan.overview_effort` (high). New `user_config/plan.rs` (`PlanSection`, getters
`plan_document_summary_model()` etc., `plan_value_of`, `plan_toml`), project tier
`fs/work_dir/config_sections/plan_config.rs` (`PlanConfig`, `read_plan_config(work_dir)`), `workspace::backs` arm
for section `plan`, dashboard fixture rows. Resolver `loom::planning::summary::config::SummaryModels::resolve(project: &PlanConfig, user: &UserConfig) -> SummaryModels { document_model, document_effort, overview_model, overview_effort }`
plus `SummaryModels::load(root: &Path) -> Result<SummaryModels>` (reads `.loom/work/config.toml` when present).
Plan 8 appends its four keys to the same KEYS/section/project-tier modules after these.
