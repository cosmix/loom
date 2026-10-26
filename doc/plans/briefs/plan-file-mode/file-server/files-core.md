# file-server / files-core: save, text store and pins, validation, routes; spec for codex units F3-F5

Tier: opus, Claude lane (the save route is a write path into the repository). Wave 1, after
`verify-split` and codex F1. Read `doc/plans/briefs/plan-file-mode/common.md` first. Spec:
`doc/product/section_06.md` "The navigation tree", "How plans render", "Viewing and editing",
"What the server does on save", "Plan validation"; `doc/product/section_12.md` "Paths in requests",
"Symlinks under `doc/`", "Saves", "Paths the server reads that sessions can write".

## Files you own

`loom/src/plan_server/files/{mod.rs, save.rs, texts.rs, validate.rs}` and their tests
(`files/tests/{mod.rs, save.rs, texts.rs, validate.rs, routes.rs}`), `loom/src/fs/safe_replace.rs`
(+ `loom/src/fs/safe_replace_tests.rs`, and the one `pub mod safe_replace;` line in
`loom/src/fs/mod.rs`), `loom/src/plan_server/routes.rs` (two lines: `files::routes()` and
`search::routes()` in `all_routes()`, `files::install` and `search::install` in `install_all()`).

The orchestrator has seeded `files/mod.rs` with every module line and `files/tests/mod.rs`
(see `codex-units.md`); keep those lines and add `routes()` and `install`. Write `texts.rs` FIRST, so
the codex units F4, F5 and the `doc-routes` worker that run beside you compile against them. The sections "tree.rs,
knowledge_index.rs" and "doc.rs, plan_view.rs" below are the specs of codex units F3-F5 (tree,
knowledge index, plan view data) and of the Claude `doc-routes` worker (`doc.rs`, `diff.rs`); you
call them with the signatures pinned there (`tree::build_tree`, `doc::doc_route`, `doc::doc_view`,
`plan_view::plan_view`, `knowledge_index::parse_index`, `diff::head_diff`, `diff::turn_diff`) from
`files/mod.rs` and never edit their files.

Read-only: plan 1's `loom::planning::{discover, sections, paths, project, slug, implements, pressure, briefs, metaplan, vision, watch}`;
plan 2's `plan_server::{context, routes, request, ids, authoring}`; `wire/files.rs` (F1);
`commands::plan::verify::report::{verify_text, VerifyReport}` (from `verify-split`).

## `fs/safe_replace.rs`

`pub fn replace_no_follow(root: &Path, rel: &Path, bytes: &[u8]) -> Result<()>`: open `root` with
`fs::safe_fs::safe_open_dirfd` (`safe_fs.rs:59`); open the parent directory of `rel` with the
no-follow walk (`safe_fs::open_safely`, `:247`, `O_DIRECTORY`; it uses `openat2(RESOLVE_NO_SYMLINKS)`
or a per-component `O_NOFOLLOW` walk); `fstatat(parent, name, AT_SYMLINK_NOFOLLOW)` must be a regular
file (else error: never create, never replace a symlink); create `.<name>.<16 random hex>.tmp` with
`openat(parent, tmp, O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW|O_CLOEXEC, <target's mode>)`; write all;
`fsync`; `renameat(parent, tmp, parent, name)`; `fsync(parent)`; unlink the temp on any error.
`safe_fs.rs` is 1019 lines and in the ledger: do not grow it; widen `open_safely` to `pub(crate)` only
if it is not already. Tests: a symlinked directory component, a symlinked target, a missing target,
and a target replaced by a symlink after the check (race simulated by a test hook is not needed:
assert that `fstatat` + `renameat` operate on the same parent fd) are refused or leave the link
untouched; a normal replace keeps the mode.

## `texts.rs`: `TextStore` (stored in `ctx.extensions()`)

```text
pub struct TurnId(pub String);
pub struct PinnedText { pub text: Arc<str>, pub hash: String }
pub struct TurnChange { pub path: PathBuf, pub before_hash: Option<String>, pub after_hash: Option<String> }
impl TextStore {
    pub fn load(root: &Path, project: &Project) -> Result<TextStore>;
    pub fn apply(&self, root: &Path, batch: &Batch);           // re-read added/changed, drop removed, move renamed
    pub fn current(&self, rel: &Path) -> Option<PinnedText>;
    pub fn begin_turn(&self, turn: &TurnId);
    pub fn end_turn(&self, turn: &TurnId, root: &Path, project: &Project) -> Vec<TurnChange>;
    pub fn turn_pin(&self, turn: &TurnId, rel: &Path) -> Option<PinnedText>;
    pub fn turn_end(&self, turn: &TurnId, rel: &Path) -> Option<PinnedText>;
}
```

Texts are read with `planning::discover::read_doc` (bounded, no-follow). `end_turn` re-reads every
document of `project.tree` now (no watcher lag), compares hashes with the pins, records created
(`before_hash: None`) and deleted (`after_hash: None`) files, and stores the end texts for
`turn_end`. Pins live for the process. Only `doc/` documents discovery lists; never the summary
store.

## `tree.rs`, `knowledge_index.rs`: `GET /api/tree` -> `FileTree`

Groups per section 06 "The navigation tree": Product (vision; listed sections in the vision
table's order, a listed section whose file is missing as a `gap` entry; unlisted sections by file
name); Plans (each metaplan, its entries in heading order with `gap` for an entry without a plan
file; then unlisted plans; plan entries carry state, `locked`, last `## Pressure tests` date, error
count from the validation cache; each plan's children are its brief bundle as on disk:
`common.md` first, then one `BriefFolder` per stage directory); Knowledge (`INDEX.md`, the seven
tier-1 files, one `KnowledgeFolder` per tier-2 category with each topic's blurb parsed from
`INDEX.md`'s tables `| [slug](cat/slug.md) | blurb | lines |` in `knowledge_index.rs`); Other (every
other discovered Markdown by path); Archive (done plans no metaplan names, other files under
`doc/plans/archive/`, orphan bundles from `briefs::orphan_bundles`). Titles: the document's first
`#` heading, else the file name. Built from `ctx.project()` only (plan 1's `Project`).

## `doc.rs`, `plan_view.rs`: `GET /api/doc?path=`

`path` passes `check_relpath_syntax` and must be in `project.tree` (else 404). Answer `DocView`:
text from `TextStore::current`, `hash`, kind, `sections` from `ProjectDoc.sections` (lines made
1-based, end inclusive), `editable` per the common brief, `lock` with the verbatim notice,
`too_large` when over 1 MiB (then `editable: false`, `text` is the first 1 MiB cut on a char
boundary, `truncated: true`, `size` the full byte size; the hash is still the whole file's).
Request parameters are relative to `doc/`; every `path` in a response or broadcast is
repo-relative (`plan_server::wire::paths::to_repo_rel`). For a plan file, `PlanView`: stages from
`plan::parser::extraction::extract_yaml_metadata` + `crate::yaml::from_str::<LoomMetadata>` (the
same parse `verify_text` uses), `line` of each stage from `DocSections.stages` spans, `yaml` the raw
block, `yaml_error` on a parse failure, `metadata_line` the block's first line, `implements` links
from `planning::implements::parse` (a number with no section file -> `path: None`), last pressure
date, `last_change` (the file's mtime as RFC 3339; read with the no-follow open), `validation`
from the cache (None for a locked plan).

## `validate.rs`

`pub fn validate_plan(root: &Path, rel: &Path, text: &str) -> ValidationView` = `verify_text(Some(root), &root.join(rel), text)`
mapped: errors with a `stage_id` get `line` = that stage's `- id:` line (`sections_of(text).stages`);
a YAML error gets `yaml_location` as line/column; warnings by bucket; notes; `metadata`. A missing
metadata block is `metadata: Missing`, `valid: true`, with the note "No metadata block yet; the plan
is saved as prose." (section 06: drafting prose first is normal, never an error). A validation cache
(`Mutex<HashMap<PathBuf, ValidationView>>` in `ctx.extensions()`) is filled at start and on every
batch for `PLAN-*.md` files that are not locked; each change broadcasts `PlanMessage::Validation`.

## `save.rs`: `POST /api/save?path=&hash=[&with_errors=1]`

`RouteSpec { method: Post, path: "/api/save", body: BodyKind::Text { max: 1_048_576 }, role: Role::HolderOrTake, .. }`
(the gate refuses `Content-Length` over 1 MiB with 413 before reading, and a content type other
than `text/plain; charset=utf-8` with 415). Handler calls:

```text
pub struct SaveRequest<'a> { pub path: &'a str /* relative to doc/; a caller holding a repo-relative path converts with wire::paths::to_doc_rel */, pub text: &'a str, pub opened_hash: &'a str, pub force: bool }
pub enum SaveOutcome { Saved { hash: String, validation: Option<ValidationView> }, Conflict { current_text: String, current_hash: String }, Invalid { validation: ValidationView } }
pub enum SaveRefusal { Authoring(Refusal), Path(String), NotFound, NotEditable(LockView), TooLarge, Symlink, Io(String) }
pub fn save_document(ctx: &PlanContext, browser: &BrowserId, req: SaveRequest<'_>) -> Result<SaveOutcome, SaveRefusal>;
```

Order: `ctx.require_authoring(browser, Take::TakeIfFree)`; `check_relpath_syntax`; in `project.tree`
(else `NotFound`; a save never creates a file); editable (else `NotEditable` with the notice);
`planning::paths::symlink_component(root, rel)` is `None` and the document is not `via_symlink`
(else `Symlink`); text over 1 MiB -> `TooLarge`; re-read the file now (not the cache) and compare
`normalized_hash` with `opened_hash` (mismatch -> `Conflict` with the current text and hash);
`PLAN-*.md` -> `validate_plan` on the proposed text, errors and not `force` -> `Invalid`;
`replace_no_follow(root, rel, text.as_bytes())`; return `Saved` with the new hash. Nothing else:
the watcher picks the write up. HTTP mapping: Saved 200, Conflict 409, Invalid 422, Authoring 403
(with the refusal text), Path 400, NotFound 404, NotEditable 403, TooLarge 413, Symlink 403, Io 500;
body is `SaveResult` or `{"error", "message"}`.

## `files/mod.rs`

`routes()` -> `/api/tree` (handler in `files/mod.rs` calling F4's `tree::build_tree`), `/api/doc`
(`doc-routes`' `doc::doc_route`), `/api/save`, `/api/diff/head`, `/api/diff/turn` (`doc-routes`' `diff::head_diff`,
`diff::turn_diff`). Register
`files::install` before `search::install` in `install_all()` (search reads the `TextStore`). `install(ctx)`: build `TextStore`, the validation cache; `ctx.on_batch` hook:
`TextStore::apply`, revalidate changed plans, broadcast `TreeChanged` (when paths were added,
removed or renamed, or a title or state changed) and one `DocChanged` per changed path.

## Tests

Route tests over plan 2's test support (`plan_server/tests/support.rs`: a started server on a
temp repo, cookies): save refusals for not holder (another browser holds), locked plan
(`DONE-PLAN-x.md`), brief of a running plan (`briefs/x/common.md` with `IN_PROGRESS-PLAN-x.md`),
knowledge (`loom/knowledge/INDEX.md`), summaries path, over 1 MiB (413 before the body is sent),
hash mismatch (409 with current text), symlinked document (403, target unchanged), read-only mode;
"Save with errors" (`with_errors=1` writes an invalid plan; without it 422); a missing metadata
block saves with the note; warnings never block a save; a save never creates a file (missing
path -> 404); every `LockView.notice` matches the common brief's wording; a document over 1 MiB has
`too_large`, `truncated`, the full `size`, the first 1 MiB of text and `editable: false`; every
response and broadcast path is repo-relative. Tree over a fixture `doc/`: group order; listed sections in the
vision table's order, then unlisted by file name; a listed but missing section and a metaplan entry
without a plan file as `gap` entries; a brief bundle with `common.md` first then stage folders; an
orphan bundle and a `REVIEW-PLAN-*.md` under Archive; knowledge `INDEX.md` first, the tier-1 files,
then categories with blurbs parsed from `INDEX.md`; titles from the first `#` heading else the file
name. Validation: a locked plan and a non-`PLAN-` file are not validated; a YAML error's line is
absolute in the file; a stage error points at that stage's `- id:` line; the Stages panel data and
the validation come from the same parse (a parse failure gives both `yaml_error` and the matching
error). Batches: adding a file broadcasts `tree_changed`, editing one broadcasts `doc_changed` and
(for a plan) `validation`. Pins: begin, edit a file, create one, delete one, end -> three changes
with the right hashes.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::files::` and `--lib fs::safe_replace`
pass; clippy clean.
