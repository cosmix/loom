# W3 — Dashboard Rust API: typed JSON wire + fixture

**Tier:** codex `gpt-5.6-terra`, `--effort xhigh`

**Files you own (write):**

- `loom/src/commands/status/web/config_api.rs`
- `loom/src/commands/status/web/config_api/wire.rs`
- `loom/src/commands/status/web/config_api/entries.rs`
- `loom/src/commands/status/web/config_api/apply.rs`
- `loom/src/commands/status/web/config_api/workspace.rs`
- `loom/src/commands/status/web/config_api/tests.rs`
- `loom/src/commands/status/web/config_api/tests/resolution.rs`
- `loom/src/commands/status/web/config_api/tests/updates.rs`
- `loom/src/commands/status/web/config_api/tests/typed_values.rs` (new file)
- `loom/src/commands/status/web/tests/config_api.rs`
- `web/src/api/fixtures/config.json`

**Read-only:** `loom/src/user_config/value.rs`, `loom/src/user_config/keys.rs`.

**Do not touch** `config_api/csrf.rs` or `config_api/request.rs` — they are HTTP
framing and CSRF only, with no value-shape concerns. Do not touch anything else
under `web/` (W4 and W5 own it) or `loom/src/fs/work_dir/**` (out of scope for
this plan).

**Do not run `git` at all.** The orchestrator stages and commits.

Every line number in this brief is where the symbol sat at HEAD and is a hint
only: W1 edits `keys.rs`, `value.rs` and `user_config/mod.rs` before you read
them, so anything you look up there will have moved. Anchor each edit on the
`fn`/`struct`/`const` name given beside the number — every name cited here is
unique in its file.

## Codex units

You are forwarded as four units, in this order, one forward each. A unit must be
completable from this brief alone inside the wrapper's 540 s deadline; the next
forward starts against the tree the previous one left. An exit 124 means the
unit was too large — re-split the remainder into smaller interface-pinned units
rather than re-forwarding the same one.

| Unit | Files written | Steps |
| --- | --- | --- |
| `w3a-wire-entries` | `config_api/wire.rs`, `config_api/entries.rs` | Task 1's `ConfigKind` rename plus the `String` arm and the `From<&ValueKind>` impl; Task 1's value-field retyping and doc comments; Task 3 |
| `w3b-handlers-workspace` | `config_api.rs`, `config_api/workspace.rs`, `config_api/apply.rs` | Task 2; Task 4; Task 5 |
| `w3c-existing-tests` | `config_api/tests.rs`, `config_api/tests/resolution.rs`, `config_api/tests/updates.rs` | Task 6's "Update in place" list; the `updates.rs` POST-body retyping; the `resolution.rs` value assertions |
| `w3d-typed-values-fixture` | `config_api/tests/typed_values.rs` (plus its one `mod typed_values;` line in `tests.rs`), `loom/src/commands/status/web/tests/config_api.rs`, `web/src/api/fixtures/config.json` | Task 6's eight new cases; the round-trip fix in `status/web/tests/config_api.rs`; regenerate the fixture LAST |

The tasks below stay the detailed reference; the table only says which forward
does which. `w3c` and `w3d` both touch `config_api/tests.rs`, which is safe only
because they are the same worker running in order: `w3d` adds the single
`mod typed_values;` line and nothing else there. Declaring the module in `w3c`
instead would leave a tree that does not compile between the two forwards.

`w3b` compiles against what `w3a` wrote, and `w3c`/`w3d` against both, while
your source-graph lookups answer from the published base layer rather than from
your own edits. The surface to compile against is Task 1's `wire.rs` block plus:

```rust
// loom/src/commands/status/web/config_api/entries.rs, after w3a
pub(super) fn built_in(spec: &KeySpec) -> ConfigValue;
// entry() and effective() thread ConfigValue where they threaded String

// loom/src/commands/status/web/config_api/workspace.rs, after w3b
fn section_key(section: Option<&toml::Value>, spec: &KeySpec) -> Option<ConfigValue>;
fn section_readable(section: &toml::Value, name: &str) -> bool;
fn resolve(spec: &KeySpec, section: Option<&toml::Value>)
    -> Option<Result<Option<ConfigValue>>>;
impl Workspace { pub(super) fn value_of(&self, spec: &KeySpec) -> Result<ConfigValue>; }
```

## Why you own the JSON fixture

`web/src/api/fixtures/config.json` is regenerated from a payload the Rust test
builds — the procedure is in the doc comment on
`the_config_fixture_matches_a_real_payload` (`config_api/tests.rs:181-191`).
Only this side can produce it, so it is yours even though it lives under
`web/`. W4 and W5 read it.

## The contract W1 is writing (quote, not lookup)

Your source-graph lookups answer from the published base layer and **cannot see
W1's edits**. Compile against this.

```rust
// loom/src/user_config/keys.rs
pub enum ValueKind {
    Bool,
    Number,                        // renamed from U32
    Enum(&'static [&'static str]),
    String,                        // new: free text
}

// loom/src/user_config/value.rs  (re-exported as crate::user_config::ConfigValue)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue { Bool(bool), Number(u32), Text(String) }

impl ConfigValue {
    pub fn parse(kind: &ValueKind, name: &str, raw: &str) -> anyhow::Result<Self>;
    pub fn from_toml_value(kind: &ValueKind, name: &str, value: &toml::Value)
        -> anyhow::Result<Self>;
    pub fn checked(self, kind: &ValueKind, name: &str) -> anyhow::Result<Self>;
    pub fn to_toml_edit(&self) -> toml_edit::Value;
}
impl std::fmt::Display for ConfigValue;

// loom/src/user_config/mod.rs
impl UserConfig { pub fn value_of(&self, spec: &KeySpec) -> (ConfigValue, Origin); }
pub fn set(spec: &KeySpec, value: ConfigValue) -> anyhow::Result<(ConfigValue, ConfigValue)>;
pub fn unset(spec: &KeySpec) -> anyhow::Result<(ConfigValue, ConfigValue)>;
```

`crate::fs::work_dir::insert_key` is UNCHANGED and still takes a
`toml_edit::Value`. Call it with `value.to_toml_edit()`.

## Task 1 — `wire.rs`

```rust
pub enum ConfigKind {          // #[serde(tag = "type", rename_all = "lowercase")]
    Bool,
    Number,                    // was U32; serializes as {"type":"number"}
    Enum { variants: Vec<String> },
    String,                    // new; serializes as {"type":"string"}
}
```

Update `impl From<&ValueKind> for ConfigKind` (`:143-153`) for both the rename
and the new variant.

Every value field becomes `ConfigValue`, which is what turns the wire native:

- `ConfigEntry.default: ConfigValue`
- `ScopeValue.value: ConfigValue`
- `EffectiveValue.value: ConfigValue`
- `ConfigUpdated.old: ConfigValue`, `ConfigUpdated.new: ConfigValue`
- `ConfigUpdate.value: Option<ConfigValue>` — the POST body. `#[serde(default)]`
  stays: absent still means "unset this key".

`ConfigEntry`, `ScopeValue` and `EffectiveValue` derive `PartialEq`/`Eq`, and
`ConfigValue` does too, so the fixture equality assertion keeps working.

Update the doc comments that say "as a string" or "the rendered value" — after
this change `value` is the value, not its rendering. `ConfigUpdate.value`'s
comment should say the value is parsed against the key's `ConfigKind` and
revalidated server-side.

## Task 2 — `config_api.rs`, the POST path

`parse_value` (`:134-149`) currently takes `Option<&str>` and calls
`spec.parse(raw)`. It now takes `Option<ConfigValue>` — serde already decoded
the JSON scalar — and revalidates:

```rust
match value {
    Some(value) => value.checked(&spec.kind, spec.name).map(Some).map_err(invalid),
    None => Ok(None),
}
```

`checked` is what closes the hole the untagged `Deserialize` opens: it can tell
a JSON bool from a number from a string, but knows nothing about an `Enum`'s
vocabulary or whether this key accepts that shape at all. **The call must be
`.checked(`** — the plan's wiring check greps for it as proof the handler
revalidates rather than trusting the body.

The project-scope guard at `:139-144` is unchanged and still runs first.

`apply_update` (`:109-130`) changes only in the type it threads:
`request.value` is `Option<ConfigValue>`, passed by value rather than
`as_deref()`.

## Task 3 — `entries.rs`

- `built_in(spec) -> ConfigValue` (`:76-78`) — `UserConfig::default().value_of(spec).0`
  already yields one.
- `entry()` (`:34-66`) and `effective()` (`:90-104`) thread `ConfigValue`
  instead of `String`; the `.clone()` calls stay.
- Nothing else changes. This file derives everything from `KEYS` and must keep
  doing so — no second table of kinds.

## Task 4 — `workspace.rs`, and the misreport you are fixing

This is the substantive bug in your territory. `section_key` (`:161-166`) reads
a project-tier value with `toml::Value::as_str` and trusts any string it finds.
The daemon does not: `[models]`/`[pressure]` are read through serde structs
with `Option<String>` fields and `#[serde(deny_unknown_fields)]`
(`models_config.rs:22-34`, `pressure_config.rs:14-24`), a section that fails to
deserialize is dropped whole (`read_models_config`, `models_config.rs:82-90`;
`read_pressure_config`, `pressure_config.rs:92-100`), and a string outside the
key's vocabulary is dropped per key with a `tracing::warn!`
(`allowed_value`, `config_sections/allowed.rs:12-28`). Either way the key falls
through to the user tier. So a `.loom/work/config.toml` holding
`standard_model = "claude-opus-5"` is shown TODAY as the project value in force
while the daemon ignores it — the settings page disagrees with the daemon,
which `tests/resolution.rs`'s own doc comment calls worse than no settings page.

**The policy: the dashboard resolves a project value exactly as the daemon
does.** An invalid value is not in force, so it is not shown as in force; it
never fails the payload and never blocks a write that repairs it. Concretely:

```rust
/// The section's own value for `spec.field`, parsed against the key's kind —
/// `None` when the section omits it, is absent, or holds something the daemon
/// ignores. Mirrors the daemon's reading so the page never shows a value as in
/// force that `resolve_stage_model_effort` / `read_pressure_config` drop:
/// a section `section_readable` rejects drops every key in it, and a value
/// `ConfigValue::from_toml_value` rejects drops that one key, with a warning
/// naming the key.
fn section_key(section: Option<&toml::Value>, spec: &KeySpec) -> Option<ConfigValue>

/// Whether the daemon's serde struct for `name` (`[models]`/`[pressure]`)
/// would deserialize `section`: a table whose every entry is a registry key of
/// that section holding a TOML string. The registry's `[models]` and
/// `[pressure]` keys are exactly those structs' fields (8 and 6), which is
/// what makes `KEYS` the right list to check against.
fn section_readable(section: &toml::Value, name: &str) -> bool
```

- `section_readable`: `section.as_table()` is `Some`, and every
  `(field, value)` in it has a `KEYS` entry with `section == name` and
  `field == field`, and `value.is_str()`.
- `section_key`: return `None` unless `section_readable(section, spec.section)`;
  then take `section.get(spec.field)` and run
  `ConfigValue::from_toml_value(&spec.kind, spec.name, value)`. On `Err(error)`,
  `tracing::warn!(key = spec.name, error = %error, "workspace config value is invalid; falling through to the user tier as the daemon does")`
  and return `None`. `from_toml_value` stays the one parser; the arm keeps the
  `Enum` invariant (a `Text` built for an `Enum` key is always a listed
  variant) because an off-list string never becomes a `ConfigValue`.
- `resolve(spec, section) -> Option<Result<Option<ConfigValue>>>` (`:136-151`).
  Its `terminal.backend` arm wraps the backend as
  `ConfigValue::Text(kind.to_string())`; its `context.ceiling_tokens` arm wraps
  the ceiling as `ConfigValue::Number(config.ceiling_tokens)`. Keep the two
  exact-name arms name-matched and the `_ => match spec.section` fallback
  section-matched, for the reason the existing doc comment gives. The fallback
  arm keeps its current shape, `Some(Ok(section_key(section.as_ref(), spec)))`.
- `Workspace::value_of(&self, spec) -> Result<ConfigValue>` (`:103-108`). Its
  only error sources stay the two exact-name arms, as at HEAD.
- `backs` (`:155-157`) and `shadows` (`:113-118`) are unchanged in body. An
  invalid value resolves to `Some(Ok(None))`, so `shadows` is false for it —
  correct, because the daemon does not honour it either.
- `has_key` (`:92-97`) is unchanged: it still reports that the FILE sets the
  key, so an invalid value shows as `project.set: true` carrying the inherited
  value. That is the repair affordance — the dialog offers to clear a set key.

### What the operator sees when a project value is wrong

The page loads. The key's project scope reads `set: true` with the user tier's
value, the effective value is whatever the daemon actually uses, and the loom
log carries the warning naming the key. A POST that overwrites or clears the key
succeeds: `apply::project` reads `old` through the same `value_of`, which no
longer fails for these values, so `ConfigUpdated.old` stays a plain
`ConfigValue` (the inherited value, which is what that scope resolved to).

Out of scope and unchanged: `[terminal] backend = 42` still fails the whole
payload and a POST to that key, because `TerminalConfig::backend_from_section`
(`loom/src/fs/work_dir/config_sections/terminal_config.rs:32-42`) errors and
lives in `fs/work_dir/**`, which this plan does not touch. Do not add a
per-entry error field to the wire either — that is new surface for W4 and W5.

Do NOT change `loom/src/fs/work_dir/config_sections/**`. Those serde structs
feed `resolve_stage_model_effort` and the daemon; typing their fields is
explicitly out of scope for this plan.

## Task 5 — `apply.rs`

`apply` (`:23-40`) and `project` (`:47-65`) take `Option<ConfigValue>` and
return `Result<(ConfigValue, ConfigValue)>`. Inside `project`, the `insert_key`
call at `:57` becomes
`insert_key(doc, spec.section, spec.field, value.to_toml_edit())?`, and the
local at `:53` — `let mut old_new: Option<(String, String)> = None;` — becomes
`Option<(ConfigValue, ConfigValue)>`. The lock discipline — capturing old and
new inside the same `update_config` hold — is unchanged, for the reason the
existing doc comment gives.

## Task 6 — tests, and regenerating the fixture

`config_api/tests/updates.rs` is 366 lines against a 400-line ceiling. Put new
cases in a new `config_api/tests/typed_values.rs` declared from `tests.rs`,
rather than growing `updates.rs`.

Update in place:

- `tests.rs:118-131` pins wire enum variants against `crate::claude::CLAUDE_MODELS`
  — still valid, check it compiles.
- Every test constructing a `toml_edit::Value` to hand to `crate::user_config::set`
  (e.g. `tests.rs:195` uses `toml_edit::Value::from(false)`) becomes
  `ConfigValue::Bool(false)`. Note the distinction: the `scratch.write_project`
  calls keep taking a `toml_edit::Value`, because those go through `insert_key`.
- **`tests/updates.rs` is the largest single edit in this territory, and the
  compiler will not find any of it.** Every POST body there carries `value` as a
  string literal inside raw-string JSON, so nothing type-checks it. Retype each
  to the shape the key's kind accepts — `"640000"` becomes `640000`, `"false"`
  becomes `false` — at `:40`, `:54`, `:72`, `:93`, `:122`, `:158`, `:354` and
  `:358`. Enum keys keep their quotes. Then retype the assertions that compare
  against a `&str`: `:105` (`updated.entry.effective.value`), `:315`
  (`updated.old`), `:362` and `:364`.
- One test in that file INVERTS.
  `a_malformed_body_is_rejected_without_naming_a_path` (`:289-302`) feeds four
  bodies and asserts a 400 for each. The fourth is
  `{"scope":"user","name":"update.check","value":true}`, which is a 400 today
  only because `ConfigUpdate.value` is an `Option<String>`. Under the typed wire
  that is the correct body for a `Bool` key and returns 200. Move it out of the
  malformed loop, keep it as a success case, and put a body the typed wire still
  rejects in its place — `"value":{}` deserializes into no `ConfigValue` variant.
- `an_invalid_value_returns_the_registrys_own_message` (`:199-221`) pins the
  exact 400 text at `:207-210`:

  ```text
  context.ceiling_tokens: "abc" is not a u32 (expected a non-negative integer)
  terminal.backend: "kitty" is not one of the expected values: native, tmux
  ```

  **This test must keep passing unchanged**, wording and all. Its 400 now comes
  from `checked` rather than from `parse`, and `checked` builds its message with
  `let raw = self.to_string();` and the same `{raw:?}` interpolation, so the text
  is byte-identical. The `Number` arm keeps saying `is not a u32 (expected a
  non-negative integer)` even though the `ValueKind` variant is renamed: the
  phrase describes the accepted range, and `web/src/components/settings-dialog.test.tsx:153`
  mirrors it. If that test needs its expected string edited, `checked` is wrong —
  fix `checked`, not the test.
- `loom/src/commands/status/web/tests/config_api.rs` is yours and appears in no
  other task. `a_valid_write_round_trips_through_a_get` (`:251-276`) POSTs
  `"value":"900000"` at `:261` for a `Number` key, which is now a 400, so `:265`
  (`starts_with("HTTP/1.1 200")`) fails: send the JSON number `900000` and
  change `:274` to assert `== 900000` rather than `== "900000"`. Nothing else in
  that file needs an edit — the other bodies either fail at the Origin/CSRF gate
  before the body is parsed (`:110`, `:130`) or send an enum, which stays a JSON
  string (`:146`, `:154`, `:176`, `:230`, and the `"new":"tmux"` assertion at
  `:248`).
- `tests/resolution.rs` pins the web API's resolution against the runtime
  readers. Its assertions compare values — update them to `ConfigValue`. **The
  pinning itself must not weaken**: the whole point of that file, per
  `workspace.rs`'s own doc comment, is that a settings page disagreeing with the
  daemon is worse than no settings page.

New coverage in `tests/typed_values.rs`. The file name and these eight function
names are pinned by the stage's `after_stage` greps — use them verbatim, a
rename turns a gate red:

- `a_bool_key_ships_a_json_boolean` and `a_number_key_ships_a_json_number` — the
  payload carries a JSON boolean for `update.check` and a JSON number for both
  `Number` keys. Assert against `serde_json::Value` types, not against rendered
  text.
- `a_string_for_a_bool_key_is_rejected_by_checked` — a POST of `"false"` (the
  string) for `update.check` is a 400. This is the case that proves the handler
  revalidates the body instead of trusting serde's untagged decode.
- `a_negative_number_is_rejected_before_checked` — a POST of `-1` for
  `context.ceiling_tokens` is a 400. Assert the status only, not the message: a
  negative or fractional JSON number matches none of `ConfigValue`'s three
  variants, so `serde_json::from_slice::<ConfigUpdate>` fails on the whole body
  and answers with serde's untagged-enum wording before `checked` ever runs.
  That is accepted for this stage and documented in the plan; the dashboard never
  sends such a number, because W5's `NumberField` commits a number only when the
  draft is decimal `u32` syntax within range and sends the raw string otherwise.
- `the_string_kind_serializes_as_type_string` — `ConfigKind::from(&ValueKind::String)`
  serializes as `{"type":"string"}`. No registry key uses that kind, so this
  assertion is the only thing that executes the new wire arm. The literal
  `ValueKind::String` must appear in this file; a gate greps for it.
- `an_off_list_project_value_falls_through_like_the_daemon` — a project
  `[models]` section holding `standard_model = "claude-opus-5"`: the payload
  builds (no error); `models.standard_model`'s project scope has `set: true`
  and the user tier's value; its effective source is not `project`; and its
  effective value equals `resolve_stage_model_effort(&work, StageType::Standard, None, None).0`
  on the same tree (the `tests/resolution.rs` pinning pattern).
- `a_non_string_project_value_drops_its_section_like_the_daemon` — a project
  `[models]` section holding `standard_model = 42` AND
  `standard_effort = "low"`: the payload builds; neither key's effective source
  is `project`; and `models.standard_effort`'s effective value equals
  `resolve_stage_model_effort(&work, StageType::Standard, None, None).1`. The
  sibling key is the point: the daemon drops the whole section, so the page
  must not show `low` as in force.
- `an_invalid_project_value_can_be_replaced_and_cleared` — starting from each
  of the two files above in turn: a project-scope POST of `"opus"` for
  `models.standard_model` is a 200 whose `new` is `"opus"` and whose `old` is
  the user tier's value; a following project-scope POST of `null` is a 200 and
  the file no longer holds `standard_model`. This is the regression the strict
  alternative would ship — a read that errors before the write can run.
Three more cases belong in the same file under names of your own choosing, since
no gate greps for them: a POST of a correctly typed value succeeds for each
kind; a POST of `42` for `terminal.backend` is a 400 reporting the registry's
own `is not one of the expected values: native, tmux`; and a POST of a string
that is not a listed variant is a 400.

**Regenerate `web/src/api/fixtures/config.json` last**, after the payload shape
is final. Follow the procedure in the doc comment on
`the_config_fixture_matches_a_real_payload`: print the payload that test builds,
write it with `serde_json::to_string_pretty` plus a trailing newline. The
scenario is the four lines above the assertion (`update.check` set false at user
scope, `context.ceiling_tokens` set to 900000 at project scope), and nothing but
`csrf_token` varies between runs — keep the existing token string in the fixture
so only value shapes change.

After regenerating, the fixture must satisfy both of these (the orchestrator
runs them as acceptance):

```text
jq -e '[.entries[] | select(.name == "update.check") | .user.value | type] == ["boolean"]' web/src/api/fixtures/config.json
jq -e '[.entries[] | select(.kind.type == "number") | .user.value | type] | unique == ["number"]' web/src/api/fixtures/config.json
```

## Size limits

A function over 50 lines is a hard gate, not a style note:
`loom/tests/maintainability/scanner.rs:6-7` sets `FILE_LINE_LIMIT = 400` and
`FUNCTION_LINE_LIMIT = 50`, measured against an exact ledger in
`loom/maintainability-baseline.txt`, and no file in your territory has an entry
there today — so any breach is a new violation that fails the stage. Split
rather than grow. `updates.rs` is 366 lines and `config_api/tests.rs` is 301,
which is why the new cases go in their own file.

## Acceptance for your slice

Run at most ONE scoped check, once, and skip it entirely if the build is cold —
the main agent compiles, tests and lints, and a cold rebuild of the library eats
the unit deadline before you have reported anything:

```text
cargo test --manifest-path loom/Cargo.toml --lib commands::status::web::config_api::
```

Observed at HEAD: `config_api::` 35 passed. Do not widen the filter to
`commands::status::web::` — that is the whole web module on a tree where W1 has
just changed `value_of`'s signature. Do not run the full suite, clippy, fmt, or
the web gate. `tests::config_api` (13 passed at HEAD) is the main agent's to
run.

## Report

Files changed, the final JSON shape per kind, whether the fixture regenerated
cleanly, what the `section_key` fix changed in observable behaviour, anything
unresolved.
