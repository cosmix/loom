# W1 (bridge-core): chat model, wire types, fixtures, attachments

Read `doc/plans/briefs/feat-planner-07-session-bridge/common.md` first ("The chat model", "Attachments").
You run in wave 3, after codex units C1 (attachment types) and C2 (model types) wrote their files
(`codex-units.md` in this directory). Lanes: codex units C1, C2, C6-C10, C12 and C13 own the model
types, the attachment types, the wire enums, the four wire fixtures and their tests; their
steps 1-4 below stay as the specification they implement. You own the attachment block.

## Files you own (write only these)

- `loom/src/plan_session/attachments/format.rs`, `loom/src/plan_session/attachments/parse.rs`,
  `loom/src/plan_session/attachments/resolve.rs`, `loom/src/plan_session/attachments/tests.rs`
  (C1 created them holding only a purpose line and declared them in `attachments/mod.rs`)
- `loom/tests/fixtures/plan_session/attachments/` (one `<case>.json` per case holding the input
  `{"text": .., "attachments": [..]}` and `<case>.txt` holding the exact delivered text
  `format_message` must produce, byte for byte; plan 9's TypeScript port of `format_message` is
  tested against the same files, so never normalise them. Cases: at least a two-entry block with
  lines, a stage address, a summary entry with rests-on, a locked entry, a changed-since-selected
  entry, a quote cut at 2,000 characters with multi-byte characters near the cut, a user text
  containing a literal `<loom-attachments>` line, and a malformed block)

Do not edit `attachments/mod.rs`, `model/`, `plan_server/wire/` or the web fixtures (codex units).

## Steps

1. `model/mod.rs`: the types verbatim from common.md, `pub`, with doc comments naming the section
   08/07 rule each field serves (one line each). Item ids are plain `String`s.
2. `plan_server/wire/chat.rs`: the wire enums verbatim; `use crate::plan_session::model::*` and
   `crate::plan_session::attachments::AttachmentInput`.
3. Fixtures: hand-write the four JSON files so that every enum value of every model and wire enum
   appears at least once (one `Item` of every type, every `ToolStatus`, `PermissionStatus`,
   `DenyReason`, `NoticeKind`, `Delivery`, `Origin`, every `ChatDelta` op, every `ChatCommand`,
   every `ChatFrame`). Realistic values: a session id is a uuid, times RFC 3339 with `Z`, a tool
   `input` is pretty JSON text, a `patch` is unified-diff text.
4. Parity tests in `plan_server/wire/tests_chat.rs`, mirroring plan 2's pattern
   (`plan_server/wire/tests.rs`, which mirrors `commands/status/web/model_tests.rs`): for each
   fixture `<name>_fixture_matches_serde_output` (`include_str!("../../../../web/src/plan/api/fixtures/<name>.json")`,
   deserialize, serialize, compare `serde_json::Value`s) and `<name>_fixture_deserializes`, plus a
   test per enum that every variant appears in the fixtures (serialize each variant, search the
   fixture text for its tag value). Confirm the `include_str!` relative path from the file's
   location before writing it.
5. Attachments: `format.rs` (`format_message`), `parse.rs` (`parse_message`), `resolve.rs`
   (`resolve`), exactly as common.md "Attachments". Character counting with `chars()`; never slice
   a `&str` by a byte index you did not get from `char_indices`. `resolve` uses plan 1's
   `planning::paths::check_relpath_syntax` (input relative to `doc/`, output repo-relative) and
   `Project` (`tree.get`, `content_hash`, `locked`); `AttachmentError` variants: `Path(String)`,
   `NotADocument`, `QuoteTooLarge { bytes }`, `RestsOn(String)`, `SentenceId`, each with a
   `Display` message. `AttachmentInput.path` and every `rests_on` entry are repo-relative
   (`doc/...`, common.md "Wire path rule"); `resolve` refuses a path without the `doc/` prefix, a
   `rests_on` entry with a newline, a missing `#`, or a path that fails `check_relpath_syntax` or
   discovery, and a summary input whose `summary_sentence` differs from its `anchor` or is not
   `s-` + 8 lowercase hex.
6. Tests (`attachments/tests.rs`): every fixture under `loom/tests/fixtures/plan_session/attachments/`
   parsed and compared, `format_message(parse_message(x)) == x` for every well-formed fixture, and
   `parse_message(format_message(t, a)) == (t, a)` for generated cases including empty text, CRLF
   in the user's text, a quote with a line `> ` of its own, and 10 attachments. `resolve` on a real project fixture with a sealed summary file: an unchanged summary selected with
   its store file's `normalized_hash` resolves with `changed_since_selected: false`, the same after
   the file changed resolves `true`, and a missing summary file is `NotADocument`
   (px-seams-R02); an input->resolved fixture pair for a summary entry
   (`loom/tests/fixtures/plan_session/attachments/resolve-summary.{input,resolved}.json`: the input
   as plan 9 builds it from plan 3's summary draft, the resolved `Attachment` with repo-relative
   `rests_on`), a `rests_on` entry with a newline, one without `doc/`, a locked
   plan, a knowledge file, a brief of a running plan, a summary path, a path with `..`, a
   non-document, a stale selected hash.

## Proof

`cargo test --lib plan_session::model:: plan_session::attachments:: plan_server::wire::` (run the
three filters as separate `cargo test --lib <filter>` calls) and
`cargo clippy --all-targets -- -D warnings` from `loom/`. Report what changed and anything
surprising (`loom memory note` too).
