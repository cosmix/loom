# Plan `feat-planner-01-documents`: common brief

Read this first; then your stage's common brief (if any) and your own brief. This file is
authoritative for the public API of `loom::planning`: later plans (2 to 11) are written against
these exact names and signatures. Change a name or signature only by recording a
`loom memory decision` naming the change and why, and report it to the orchestrator.

## The spec you implement

- `doc/product/section_01.md` (whole): steps, detection rules, offers, skipped steps.
- `doc/product/section_03.md` (whole): discovery, kinds, slugs, briefs, run reviews,
  `Implements:`, cross-references, headings as anchors.
- `doc/product/section_04.md` "The watcher" and "Hashing" only.
- `doc/product/section_09.md` "Checking conventions: `loom plan check`".
- `doc/product/section_02.md` "Runtime state".
- `doc/product/section_12.md` "What a session may write" (the locked-set row), "Three layers
  enforce the table" (layer 3 bullets), "Paths in requests", "Symlinks under `doc/`".

Read your sections in full before writing code. Where this brief and the spec differ, this
brief wins (it records decisions the spec left open) and you note the difference with
`loom memory note`.

## Rules for every worker

- Code lives under `loom/src/planning/` (module `loom::planning`, declared `pub mod planning;` in
  `loom/src/lib.rs`). Every file at most 400 lines and every function at most 50 lines
  (`cargo test --test maintainability` enforces both against `loom/maintainability-baseline.txt`,
  an exact ledger that also fails when a listed file shrinks; this plan adds no row).
  Split a module into a directory (`foo.rs` plus `foo/bar.rs`) before it reaches the limit.
- Unit tests go in a `tests.rs` (or `tests/<name>_tests.rs`) beside the module, declared
  `#[cfg(test)] mod tests;`. The frozen contract files (`loom/tests/planning_*_contracts.rs`,
  `loom/src/plan/inputs/retired_tests.rs`) are never edited.
- Every open or listing of a path under `doc/` goes through `crate::fs::safe_read`
  (`read_bounded` `safe_read.rs:48`, `read_to_string_bounded` `:134`, `open_regular_no_follow`
  `:152`, `list_dir_no_follow` `:197`, `EntryKind`) and never `std::fs::read*`/`read_dir`/`File::open`,
  because sessions can write `doc/` (`doc/loom/knowledge/patterns/security-sandbox-and-hooks.md`).
  `std::fs::symlink_metadata` (lstat, no follow) and `std::fs::canonicalize` are allowed for the
  symlink-target check. The one exception is `plan::inputs::collect_inputs`, which this plan calls
  as `loom run` does, unchanged apart from the `RETIRED-` strip.
- Repo-relative paths always start with `doc/` and use `/`. The root passed to every function is the
  repository root (the directory holding `doc/`), never `doc/` itself.
- Library code returns `anyhow::Result`; no `unwrap`/`expect` outside tests; no `println!` outside the
  command module.
- Public types derive `Debug, Clone, PartialEq, Eq` where possible and `serde::Serialize` where the
  ledger says JSON carries them (`#[serde(rename_all = "snake_case")]` on enums, tagged enums
  `#[serde(tag = "kind", rename_all = "kebab-case")]` for `Offer`).
- Every public item gets a `///` doc comment (the gate runs `cargo doc` with `-D warnings`).
- No new dependency: `sha2`/`hex` through `context::source_graph::body_hash`, `serde`,
  `serde_json`, `nix` and `libc` are already in `loom/Cargo.toml`.
- Record mistakes, decisions and surprises with `loom memory note|decision` at once; never
  `loom knowledge`, never Claude Code auto-memory. Never run git commands that change state.

## Public API (authoritative)

All under a new top-level library module `loom::planning` (`loom/src/planning/`, `pub mod planning;` in
`loom/src/lib.rs`). Paths are repo-relative `PathBuf`s (`doc/plans/PLAN-x.md`) unless noted. Rust
contracts live in `loom/tests/planning_*_contracts.rs`.

**Runtime paths** [`document-model`] `loom::planning::runtime` (pure path computation, creates nothing):
- `pub fn project_key(root: &Path) -> Result<String>`: `<dir name>-<first 12 hex of sha256(canonical root)>`;
  dir name sanitised to `[A-Za-z0-9._-]`, other chars `_`.
- `pub struct RuntimePaths { pub planning_root, pub dir, pub lock, pub drafts, pub claims, pub summaries_lock, pub summary_cost, pub sessions: PathBuf }` (`planning_root` = `<loom user dir>/plan`, the parent plan 6 deny-reads)
- `pub fn runtime_paths_under(loom_dir: &Path, root: &Path) -> Result<RuntimePaths>` (pure; tests), `pub fn runtime_paths_from(state: Option<&Path>, session: Option<&str>, loom_dir: &Path, root: &Path) -> Result<RuntimePaths>` and `pub fn runtime_paths(root: &Path) -> Result<RuntimePaths>` (= `runtime_paths_from(env LOOM_PLAN_STATE, env LOOM_PLAN_SESSION, loom user dir, root)`, the in-session CLI's resolver). Inside a planning session plan 7 sets `LOOM_PLAN_STATE=<runtime dir>` and drops `LOOM_HOME`; when it is set it must be absolute, a real directory (lstat, no symlink), owned by the current uid, named `project_key(root)`, accompanied by a well-formed `LOOM_PLAN_SESSION`, and its parent must NOT be writable by this process (`access(parent, W_OK)` fails: the sandbox makes the real planning root read-only, while any directory a session made for itself sits under a writable parent), else an error naming `LOOM_PLAN_STATE`; it then IS `dir` (and `planning_root` its parent). The SERVER never honours it: `pub fn server_runtime_paths(root) -> Result<(RuntimePaths, Option<OsString>)>` resolves without the variable and returns it when set, so plan 2 prints a warning. Unset: `dir = <loom user dir>/plan/<key>` where the loom
  user dir is `crate::user_config::config_path()?.parent()` (so `LOOM_HOME` redirects it, as tests need);
  `lock`, `drafts/`, `claims/`, `summaries.lock`, `summary-cost.jsonl`, `sessions/` under it. Plan 2 creates the
  directory 0700; plan 4 creates `drafts/`, `claims/` on demand when no server ran.

**Kinds and ids** [`document-model`] `loom::planning::kinds`:
- `pub enum DocKind { Vision, ProductSection { number: u32 }, ProductNote, Metaplan, Plan { slug: String, state: PlanState }, Brief { slug: String }, RunReview { slug: String }, Knowledge, Other }`
- `pub fn classify(rel: &Path) -> DocKind` (path and name alone).
- `pub struct DocId(pub String)`; `pub fn doc_id(rel: &Path, kind: &DocKind) -> DocId`: `product/index`,
  `product/section_<nn>` (two-digit zero pad), `plan/<slug>`, else the path under `doc/` (e.g. `plans/index.md`).

**Plan slugs** [`document-model`] `loom::planning::slug`:
- `pub enum PlanState { Written, Running, Done, Retired }` (derive `Ord` in that order; "most advanced wins").
- `pub fn plan_slug(file_name: &str) -> Option<String>`: strips one of `IN_PROGRESS-`/`DONE-`/`RETIRED-`, then requires `PLAN-` and `.md`.
- `pub fn plan_state(rel: &Path) -> Option<PlanState>`: `doc/plans/PLAN-` written, `doc/plans/IN_PROGRESS-PLAN-` running,
  `DONE-PLAN-` in `doc/plans/` or any plan spelling under `doc/plans/archive/` done, `RETIRED-PLAN-` (plans or archive) retired.
  Plans are recognised only directly in `doc/plans/` and `doc/plans/archive/`.
- `pub struct PlanEntry { pub slug: String, pub path: PathBuf, pub state: PlanState, pub strays: Vec<PathBuf> }`
- `fs::plan_lifecycle::RETIRED_PREFIX: &str = "RETIRED-"` added; `plan/inputs.rs::bundle_dir` and
  `commands/usage/discovery.rs::normalize_plan` strip it.

**Discovery** [`document-model`] `loom::planning::discover`:
- `pub struct Document { pub path: PathBuf, pub kind: DocKind, pub id: DocId, pub size: u64, pub via_symlink: bool }`
- `pub struct DocTree { pub docs: Vec<Document> /* sorted by path */, pub plans: BTreeMap<String, PlanEntry>, pub doc_dir: DocDirState }`,
  `pub enum DocDirState { Present, Missing, Symlinked }`; methods `get(&self, rel: &Path) -> Option<&Document>`,
  `by_id(&self, id: &str) -> Option<&Document>`, `plan(&self, slug: &str) -> Option<&PlanEntry>`.
- `pub fn discover(root: &Path) -> Result<DocTree>`: no-follow walk (`fs::safe_read::list_dir_no_follow`), skips
  dotfiles, `doc/loom/summaries/`, editor temporaries (`*~`, `*.swp`, `.#*`), symlinked dirs, and symlinks except
  one whose resolved target is a regular file inside canonical `doc/` and outside the summary store. Reads
  git-ignored files.
- `pub fn read_doc(root: &Path, rel: &Path) -> Result<String>`: bounded (`MAX_DOC_BYTES = 8 MiB`) no-follow read via
  `fs::safe_read`; an allowed symlink is re-resolved and its target opened no-follow from the `doc/` root.

**Heading sections, anchors, span hashes** [`document-model`] `loom::planning::sections`:
- `pub struct Span { pub start_line: usize, pub end_line: usize /* exclusive, 0-based */, pub hash: String }`
- `pub struct HeadingSection { pub level: u8, pub title: String, pub anchor: String, pub span: Span }`
- `pub struct DocSections { pub preamble: Span, pub headings: Vec<HeadingSection>, pub metadata: Option<Span>, pub stages: Vec<(String /* stage id */, Span)>, pub file_hash: String }`
- `pub fn sections_of(text: &str) -> DocSections`; `impl DocSections { pub fn find(&self, anchor: &str) -> Option<&Span> }`:
  `""` = preamble, `loom-metadata`, `stage:<id>`, or a GitHub anchor.
- `pub fn github_slug(title: &str) -> String` (no dedupe); dedupe `-1`, `-2` applied in `sections_of`.
  `## 4.2 Line Grammar` -> `42-line-grammar`.
- `pub fn normalized_hash(text: &str) -> String`: CRLF/CR -> `\n`, trailing whitespace per line dropped, then
  `context::source_graph::body_hash` (`sha256:<hex>`). Every span hash and `file_hash` use it.
- Built on `fs::knowledge::splice::{atx_heading, fence_mask, section_end}`, widened to `pub(crate)`.

**Shared path predicate** [`document-model`] `loom::planning::paths` (plans 3 save route, 6 hook):
- `pub enum LockReason { PlanState(PlanState), Archive, RunReview, BriefOfLockedPlan { slug: String } }`
- `pub struct LockedSet { .. }`; `impl LockedSet { pub fn scan(root: &Path) -> Result<LockedSet>; pub fn lock_reason(&self, rel: &Path) -> Option<LockReason>; pub fn locked_paths(&self) -> Vec<PathBuf> /* existing files, for the spawn-time layer 1/2 lists */ }`.
  Locked by NAME: `IN_PROGRESS-`/`DONE-`/`RETIRED-` plan files, everything under `doc/plans/archive/`,
  `doc/plans/REVIEW-*.md`, and the briefs of a locked plan (brief-to-plan rule: every file under
  `doc/plans/briefs/<slug>/` plus every input `plan::inputs::collect_inputs(root, plan)` lists for that plan).
  A path that does not exist yet under a locked plan's bundle dir is locked too.
- `pub enum DocArea { Locked(LockReason), Knowledge, Summaries, Doc, OutsideDoc }`;
  `pub fn classify_write(locked: &LockedSet, rel: &Path) -> DocArea` (pure, after syntax checks).
- `pub fn check_relpath_syntax(raw: &str) -> Result<PathBuf, PathSyntaxError>`: section 12 "Paths in requests"
  rules (absolute, `..`/`.`/empty components, backslash, control chars incl. NUL, > 1024 bytes); input is relative to `doc/`, output repo-relative (`doc/...`).
- `pub fn symlink_component(root: &Path, rel: &Path) -> Result<Option<PathBuf>>`: first component below `root` that is a symlink (lstat walk), `None` if none.

**Watcher** [`document-model`] `loom::planning::watch` (sections 01, 04, 05, 06 consume it):
- `pub struct Batch { pub added: Vec<PathBuf>, pub changed: Vec<PathBuf>, pub removed: Vec<PathBuf>, pub renamed: Vec<(PathBuf /* from */, PathBuf /* to */)>, pub initial: bool }`
- `#[derive(Clone, Default)] pub struct OwnWrites(..)`; `impl OwnWrites { pub fn register(&self, rel: &Path, hash: &str) }`
  — plan 4 registers the `normalized_hash` of every summary file it seals or rewrites; the watcher drops a
  changed/added summary path whose hash equals the registered one (entry consumed on match).
- `pub struct Watcher`; `impl Watcher { pub fn new(root: PathBuf, own: OwnWrites) -> Self; pub fn poll(&mut self) -> Result<Option<Batch>>; pub fn spawn(self, interval: Duration, stop: Arc<AtomicBool>, on_batch: impl FnMut(Batch) + Send + 'static) -> JoinHandle<()> }`.
  `poll` is one walk (size, mtime, inode compare; hashes only differing files); it returns the accumulated batch on
  the first walk that finds nothing new. The first walk returns `initial: true` with every file as `added`.
  The watcher walks ALL of `doc/` including `doc/loom/summaries/` (consumers filter), with discovery's skip rules
  otherwise. `spawn` default interval 1 s.

**Document parsing** [`checklist-check`] `loom::planning::{vision, metaplan, implements, pressure, xref, briefs}`:
- `vision::parse_sections_table(text) -> Result<Vec<VisionRow { number: u32, name: String, line: usize }>, VisionProblem>`
- `metaplan::is_metaplan(rel) -> bool`; `metaplan::entries(text) -> Vec<MetaplanEntry { heading: String, anchor: String, plan_file: Option<String>, slug: Option<String>, line: usize }>`
- `implements::parse(text) -> ImplementsLine { value: Implements, line: Option<usize>, duplicate_lines: Vec<usize>, bad_tokens: Vec<String> }`, `enum Implements { Unmapped, NoneDeclared, Sections(Vec<u32>) }`
- `pressure::records(text) -> Vec<PressureRecord { date: String, line: usize, text: String }>`; `open_questions(text) -> usize`
- `xref::references(doc_rel: &Path, text: &str, tree: &DocTree, sections: &dyn Fn(&Path) -> Option<DocSections>) -> Vec<Reference { form: RefForm, raw: String, line: usize, target: Option<(DocId, Option<String>)> }>`
- `briefs::plan_briefs(root, &PlanEntry, stage_ids: &[String]) -> Result<PlanBriefs { by_stage: BTreeMap<String, Vec<PathBuf>>, common: Vec<PathBuf>, other: Vec<PathBuf>, broken: Vec<String> }>`; orphan bundles via `briefs::orphan_bundles(&DocTree)`.

**Checklist** [`checklist-check`] `loom::planning::checklist`:
- `pub struct ParsedDocs { pub vision_rows: Result<Vec<VisionRow>, VisionProblem>, pub metaplans: Vec<(PathBuf, Vec<MetaplanEntry>)>, pub pressure: BTreeMap<String /* slug */, Vec<PressureRecord>>, pub locked_files: Vec<PathBuf> }`; `pub fn build_checklist(tree: &DocTree, parsed: &ParsedDocs) -> Checklist` (pure; `Project::load` builds `ParsedDocs` and calls it).
- `pub enum Step { Vision = 1, ProductSpec, Metaplan, Plans, PressureTest, Review, Run }`; `pub enum StepMark { Done, Gap, Active, Optional }`
- `pub struct Checklist { pub current: CurrentStep, pub steps: Vec<(Step, StepMark)>, pub items: Vec<ChecklistItem>, pub primary: Option<Offer>, pub secondary: Vec<Offer>, pub state: ProjectState }`,
  `pub enum CurrentStep { Step(Step), Ready, ReadyAllDone }`
- `pub enum Offer { StartVision, FixVisionTable, WriteSection { number: u32 }, AddSectionToVision { number: u32 }, StartMetaplan, WritePlan { metaplan: PathBuf, slug: String }, AddPlanToMetaplan { slug: String }, PressureTest { slug: String }, StartRun, RefineSpec, NewMetaplanRound }`
  each with `pub fn id(&self) -> String` (stable string, e.g. `write-section:05`) and `target(&self) -> Option<PathBuf>`.
- `pub struct ProjectState { pub vision_present: bool, pub sections_listed: usize, pub sections_written: Vec<u32>, pub metaplans: Vec<PathBuf>, pub plans_unlisted: usize, pub plans_by_state: BTreeMap<PlanState, usize>, pub locked_files: Vec<PathBuf> }` (plan 8's starting-point block).
- All types `serde::Serialize` with `snake_case`/kebab ids as in the JSON below.

**Loaded project view** [`checklist-check`] `loom::planning::project` (what the server holds; answers frontend's assumption):
- `pub struct ProjectDoc { pub doc: Document, pub title: String /* first H1, else file name */, pub sections: DocSections, pub open_questions: usize, pub references: Vec<Reference>, pub lock: Option<LockReason>, pub read_error: Option<String> }`
- `pub struct Project { pub root: PathBuf, pub tree: DocTree, pub docs: BTreeMap<PathBuf, ProjectDoc>, pub vision_rows: Result<Vec<VisionRow>, VisionProblem> /* Err also when no vision */, pub metaplans: Vec<(PathBuf, Vec<MetaplanEntry>)>, pub implements: BTreeMap<String /* slug */, ImplementsLine>, pub implemented_by: BTreeMap<u32, Vec<String /* slug */>>, pub checklist: Checklist, pub problems: Vec<Problem>, pub locked: LockedSet }`
- `impl Project { pub fn load(root: &Path) -> Result<Project>; pub fn refresh(&mut self, batch: &Batch) -> Result<()> /* re-reads only batch paths, recomputes derived fields */; pub fn content_hash(&self, rel: &Path) -> Option<&str> /* = sections.file_hash */; pub fn section_text(&self, rel: &Path, anchor: &str) -> Result<Option<(Span, String)>> /* re-reads the file, verifies file_hash */ }`
- `pub struct Problem { pub code: &'static str, pub path: PathBuf, pub line: Option<usize>, pub message: String }` (serde: `code`, `path`, `line`, `message`).
- Reading order for "first": `Project::vision_rows` order, metaplan heading order, then unlisted plans by file name.

**`loom plan check [--json]`** [`checklist-check`] `PlanCommands::Check { json: bool }`, `commands/plan/check.rs`:
JSON (one object, pretty-printed, stdout): `{"schema":1,"root":"<abs>","checklist":<Checklist>,"problems":[{"code":"<code>","path":"doc/...","line":<n|null>,"message":"..."}]}`.
Codes: `vision-table-unreadable`, `unlisted-section`, `missing-title`, `metaplan-entry-without-plan`,
`implements-missing`, `implements-duplicate`, `implements-broken`, `broken-reference`, `broken-brief-reference`,
`stray-plan-copy`, `doc-missing`. Exit 0 with no problems, 1 with any, 2 on an error (not a repo). Human output:
checklist then problems, one per line `<path>:<line>: <code>: <message>`. Pinned by
`loom/tests/fixtures/planning/check_schema.json` and a schema round-trip test.
