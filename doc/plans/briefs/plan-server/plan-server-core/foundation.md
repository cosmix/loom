# plan-server-core / foundation: spec for codex units C1-C6 (ids and the wire model)

Lane: codex (`codex-units.md`, units C1-C6, wave 0). Each unit owns the one file named in the
units table plus its fixtures; the orchestrator writes `plan_server/mod.rs` and `wire/mod.rs`
first. Read `doc/plans/briefs/plan-server/common.md` first.

## ids.rs

- `#[derive(Clone, PartialEq, Eq, Hash)] pub struct BrowserId(String)`: 32 lowercase hex (128 bits).
  `BrowserId::mint() -> io::Result<BrowserId>` (16 bytes from `/dev/urandom`, `hex::encode`, as
  `web_server::token::mint` does with 32); `BrowserId::parse(&str) -> Option<BrowserId>` accepts
  exactly 32 lowercase hex chars; `as_str()`. `Debug` prints `BrowserId(..)` and never the value.
- `#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)] #[serde(transparent)] pub struct PublicId(String)`:
  16 lowercase hex, `PublicId::mint()`, independent of any `BrowserId`.
- `#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct ConnId(pub u64)`.
- `pub struct ConnCtx { pub id: ConnId, pub browser: BrowserId, pub public_id: PublicId }`.
- Unit tests (`ids_tests.rs`): parse rejects 31/33 chars, uppercase, non-hex; two mints differ; `Debug` hides the value.

## Wire model (serde, `Serialize + Deserialize`, `#[serde(deny_unknown_fields)]` on structs)

`wire/message.rs`:

```text
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanMessage {          // server -> browser
    Hello(Hello),
    Presence(PresenceList),
    Authoring(AuthoringView),
    Checklist(ChecklistView),
    ServerStopping,
    Refused(Refused),
}
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {        // browser -> server
    SetName { name: String },
    Reading { path: Option<String> },
}
```

Each later plan adds variants; keep one variant per line group with a doc comment, so an added
variant is a small diff. `PlanMessage::to_frame(&self) -> String` (`serde_json::to_string`,
infallible for these types: `expect` with the reason).

**Path rule (binding for every plan):** every wire field named `path`, `target`, `paths` or holding
a document path is repo-relative and starts with `doc/` (`doc/plans/PLAN-x.md`). The only
doc-relative forms are plan 3's HTTP request parameters (`?path=`), `SaveRequest.path`, and the
browser's `/files/<path>` addresses. `wire/paths.rs` holds the two Rust helpers
`pub fn to_doc_rel(repo_rel: &str) -> Option<&str>` (strips a leading `doc/`, None otherwise) and
`pub fn to_repo_rel(doc_rel: &str) -> String`, with tests; plan 3 mirrors them in the browser.

`wire/server.rs`: `ServerInfo { project: String, project_key: String /* planning::runtime::project_key(root) */, port: u16, remote: bool, read_only: Vec<MissingPrereq>, stopping: bool, you: You }`,
`MissingPrereq { what: PrereqKind, message: String, fix: String }`,
`#[serde(rename_all = "snake_case")] enum PrereqKind { Tmux, Claude, ClaudeLogin, Sandbox }`,
`You { public_id: PublicId, name: String }`, `ShareLink { url: String, bind: BindKind, tunnel: Option<String> }`,
`enum BindKind { Loopback, Remote }`, `Refused { command: String, reason: String }`,
`Hello { you: You, server: ServerInfo, checklist: ChecklistView, presence: PresenceList, authoring: AuthoringView }`.

`wire/presence.rs`: `PresenceList { entries: Vec<PresenceEntry> }`,
`PresenceEntry { public_id: PublicId, name: String, holder: bool, away: bool, reading: Option<String>, tabs: u32 }`,
`AuthoringView { holder: Option<HolderView> }`, `HolderView { public_id: PublicId, name: String, away_since: Option<String> /* RFC 3339 UTC */, frees_at: Option<String> /* away_since + 10 min */ }`.

`wire/checklist.rs`: `ChecklistView { current: CurrentView, steps: Vec<StepView>, primary: Option<OfferView>, secondary: Vec<OfferView>, items: Vec<ItemView> }`,
`OfferView { id: String /* plan 1's Offer::id(), e.g. write-section:05 */, kind: OfferKind, label: String, target: Option<String> /* Offer::target(), repo-relative doc/... */ }`,
`#[serde(rename_all = "snake_case")] enum OfferKind { StartVision, FixVisionTable, WriteSection, AddSectionToVision, StartMetaplan, WritePlan, AddPlanToMetaplan, PressureTest, StartRun, RefineSpec, NewMetaplanRound }`
(one per plan 1 `Offer` variant), `ItemView { step: u8, text: String, path: Option<String>, offer: Option<OfferView> }`
(map plan 1's `ChecklistItem`: read its fields; `text` is its one-line description).
Labels (section 01 "What the app offers"): "Start a vision discussion", "Fix the vision's section
table", "Write section {nn}", "Add section {nn} to the vision's table", "Start a metaplan from the
product spec", "Write plan {slug}", "Add {slug} to a metaplan", "Pressure-test {slug}", "Start a
run", "Refine the spec in light of what was built", "Start a new metaplan round".
`StepView { number: u8, name: String, mark: MarkView }`, `enum MarkView { Done, Gap, Active, Optional }`,
`#[serde(tag = "kind", rename_all = "snake_case")] enum CurrentView { Step { number: u8 }, Ready, ReadyAllDone }`,
and `impl From<&planning::checklist::Checklist> for ChecklistView` (plan 1, stage `checklist-check`:
`Checklist { current: CurrentStep, steps: Vec<(Step, StepMark)>, .. }`, `Step` discriminants 1..=7).
Step names, in order: "Vision", "Product spec", "Metaplan", "Plans", "Pressure test", "Review", "Run".

## Fixtures and parity tests

Hand-write each fixture JSON (pretty, two-space indent). Each wire module carries its own parity
tests in an inline `#[cfg(test)] mod tests` (no shared `wire/tests.rs`, so each unit owns one
file), mirroring `loom/src/commands/status/web/model_tests.rs` (`fixture_matches_serde_output`:143,
`fixture_deserializes_into_web_snapshot`:155): per fixture, `include_str!("../../../../web/src/plan/api/fixtures/<name>.json")`,
deserialize into the Rust type, serialize back, compare `serde_json::Value`s; name the tests
`<name>_fixture_matches_serde_output`. `hello.json` holds a holder that is away (both time fields
set), a read-only list with one entry, and a checklist with a primary offer, one secondary offer
and two items, so every optional field is exercised. A `From<&Checklist>`
test builds a `Checklist` through plan 1's public API from a fixture tree (`planning::checklist::checklist`
over a temp `doc/` with only `doc/product/index.md`) and asserts `current == Step { number: 2 }` or
whatever plan 1's rules give; read plan 1's tests for the expected step rather than guessing.

The TS side (`web/src/plan/api/schema.ts`) and the enum-value parity check against it are written
in the next stage (`plan-web-shell`); do not create them here.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::` passes and `cargo clippy` is clean
for your files. Report the fixture list.
