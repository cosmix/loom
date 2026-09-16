# W1 — Typed value core, user config, CLI

**Tier:** sonnet (`loom-software-engineer`)

**Files you own (write):**

- `loom/src/user_config/value.rs` (NEW)
- `loom/src/user_config/keys.rs`
- `loom/src/user_config/mod.rs`
- `loom/src/user_config/parse.rs`
- `loom/src/user_config/pressure.rs`
- `loom/src/user_config/models.rs`
- `loom/src/user_config/render.rs`
- `loom/src/user_config/write.rs`
- `loom/src/user_config/tests.rs`
- `loom/src/user_config/tests/value.rs` (NEW)
- `loom/src/user_config/tests/persistence.rs`
- `loom/src/commands/config/mod.rs`
- `loom/src/commands/config/tests.rs`

**You must NOT touch:** `loom/src/commands/config/tui/**` (W2),
`loom/src/commands/status/web/**` (W3), `web/**` (W3/W4/W5),
`loom/src/fs/work_dir/**` (nobody — out of scope for this plan).

You are the foundation. W2 and W3 compile against the public surface you write,
and they are briefed with its exact text, so **do not deviate from the
signatures below**. If you believe one is wrong, implement it as specified and
say so in your report.

**Two constraints that apply to every task below.**

A function over 50 lines is a hard gate, not a style note.
`loom/tests/maintainability/scanner.rs:6-7` sets `FILE_LINE_LIMIT = 400` and
`FUNCTION_LINE_LIMIT = 50`, measured against the exact ledger in
`loom/maintainability-baseline.txt`, and this stage's acceptance runs
`cargo test --manifest-path loom/Cargo.toml --test maintainability`. No file you
touch has a ledger entry, so any breach is a NEW violation. `parse`,
`from_toml_value` and `checked` each grow an arm per kind and are the three
bodies at risk — `KeySpec::parse` is 33 lines today. Split a per-kind helper out
rather than letting one of them grow past 50.

Every line number in this brief is as of HEAD and several of your own edits move
the ones below them. Anchor each edit by the `fn`/`struct`/`impl` name given
beside the number; every name cited here is unique in its file. W2 and W3 cite
line numbers into the files you are editing, so their citations will have drifted
by the time they read them — that is expected and is not something you fix.

## Why

`KeySpec::parse` (`keys.rs:186-218`) already produces a typed
`toml_edit::Value`, so the WRITE path is typed end to end. `UserConfig::value_of`
(`render.rs:17-43`) returns `(String, Origin)` for every key regardless of kind,
so the READ path collapses to text at one function and every surface downstream
inherits a string. You are replacing that collapse with a typed value, and
adding a free-text `String` kind the registry cannot currently express.

## Task 1 — `loom/src/user_config/value.rs` (new)

Create the module and declare it from `mod.rs` (`pub mod value;` plus a
`pub use value::ConfigValue;` re-export so callers say
`crate::user_config::ConfigValue`).

```rust
/// A config value in its own type rather than its rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    Bool(bool),
    Number(u32),
    Text(String),
}
```

`#[serde(untagged)]` is what makes the JSON wire native: `Bool(true)`
serializes as `true`, `Number(24)` as `24`, `Text("opus")` as `"opus"`. Keep the
variant order exactly as above — untagged deserialization tries variants in
declaration order, and a JSON `true` must not be considered for `Number` first.

Methods, all on `impl ConfigValue`:

- `pub fn parse(kind: &ValueKind, name: &str, raw: &str) -> Result<Self>` —
  move the four match arms out of `KeySpec::parse` (`keys.rs:186-218`) verbatim,
  preserving each error message **word for word** (tests and the dashboard's
  400 body both pin this wording), and add the new arm:
  - `ValueKind::Bool` → `raw.parse::<bool>()` → `Self::Bool`, error
    `"{name}: {raw:?} is not a bool (expected true or false)"`.
  - `ValueKind::Number` → `raw.parse::<u32>()` → `Self::Number`, error
    `"{name}: {raw:?} is not a u32 (expected a non-negative integer)"`. Keep
    this arm's wording BYTE-IDENTICAL, "is not a u32" included. It names the
    accepted range rather than the Rust type, and two tests pin it:
    `config_api/tests/updates.rs:207-210` asserts the 400 body a dashboard
    client receives, and `web/src/components/settings-dialog.test.tsx:153`
    mirrors the same string in the SPA. Renaming it to match the `Number`
    variant breaks both for no gain.
  - `ValueKind::Enum(variants)` → membership check → `Self::Text`, error
    `"{name}: {raw:?} is not one of the expected values: {}"` with
    `variants.join(", ")`.
  - `ValueKind::String` → `Ok(Self::Text(raw.to_owned()))`, infallible.
- `pub fn from_toml_value(kind: &ValueKind, name: &str, value: &toml::Value) -> Result<Self>`
  — note `toml::Value`, not `toml_edit`: W3 calls this on the workspace config,
  which is parsed with the `toml` crate. A shape mismatch is an error naming the
  key and `value.type_str()`, **never a silent `None`**. For `Number`, reject a
  negative or out-of-range integer with
  `"{name}: {int} is out of range for a u32"`, matching `parse::get_u32`
  (`parse.rs:44-46`). For `Enum`, check membership with the same message
  `parse` uses.
- `pub fn checked(self, kind: &ValueKind, name: &str) -> Result<Self>` — the
  entry point for a value that arrived already-deserialized from JSON. Confirm
  the variant matches the kind (`Bool`↔`Bool`, `Number`↔`Number`,
  `Text`↔`Enum`/`String`) and, for `Enum`, that the text is a listed variant.
  On a variant/kind mismatch, produce the SAME message `parse` would have
  produced for that kind, so a dashboard client sending `42` for
  `terminal.backend` gets "is not one of the expected values: native, tmux"
  rather than a distinct second wording. The rule that makes that constructible:
  build the message with `let raw = self.to_string();` and the same `{raw:?}`
  interpolation `parse` uses. Then
  `Text("abc".into()).checked(&ValueKind::Number, "context.ceiling_tokens")`
  reproduces `parse(&ValueKind::Number, "context.ceiling_tokens", "abc")` byte
  for byte, which is what `config_api/tests/updates.rs:207-210` pins — after
  this plan that test's 400 arrives from `checked` rather than from `parse`, and
  it must keep passing unchanged. `Number(42)` against `Bool` gives
  `"42" is not a bool (expected true or false)`; `Bool(true)` against an `Enum`
  gives `"true" is not one of the expected values: ...`.
- `pub fn to_toml_edit(&self) -> toml_edit::Value` — `Bool` →
  `toml_edit::Value::from(*b)`; `Number` → `toml_edit::Value::from(*n as i64)`
  (the existing cast at `keys.rs:200`); `Text` →
  `toml_edit::Value::from(s.as_str())`.
- `pub fn to_toml_literal(&self) -> String` — the right-hand side of a TOML
  assignment. `Bool`/`Number` use `Display`. `Text` MUST go through
  `self.to_toml_edit().to_string().trim().to_owned()` so escaping and quoting
  come from `toml_edit` itself. This is the one thing keeping
  `UserConfig::to_toml_string` in lockstep with what `write.rs` puts on disk;
  a hand-rolled `format!("\"{s}\"")` drifts the moment a value contains a quote.

And:

```rust
impl std::fmt::Display for ConfigValue
```

`Bool` → `true`/`false`, `Number` → the integer, `Text` → the string
**unquoted**. This must reproduce today's `value_of` output byte for byte:
`loom config -k <key>` output is unchanged by this plan, and
`commands/config/tests.rs` pins it.

## Task 2 — `keys.rs`

- `ValueKind`: rename `U32` to `Number`, add `String`. Update the doc comment on
  each variant; `String` reads "Free text: any string the operator types."
  **The declaration lines are a gate.** Declare both variants bare, each alone
  on its line, exactly `Number,` and `String,`: the stage's
  `before_stage`/`after_stage` pair greps `^\s+String,$` in `keys.rs` (absent
  at HEAD, present after), and a comment cannot satisfy it. The literal
  `ValueKind::String` will not appear in `keys.rs` at all once your collapse of
  `KeySpec::parse` moves the last `ValueKind::`-prefixed uses into `value.rs`,
  so that text is grepped in the tests instead, and the free-text arm is proven
  by running `the_string_kind_accepts_free_text` (Task 6). The pair's other half
  greps `ValueKind::U32|^\s+U32,$` in `keys.rs` at exit 1, so the rename has to
  be a rename and not an addition beside the old variant.
- Update the two `ValueKind::U32` uses in `KEYS`
  (`update.check_interval_hours`, `context.ceiling_tokens`) to
  `ValueKind::Number`. **No key gains `ValueKind::String`** — the variant is
  infrastructure for a future key, and inventing one now is out of scope.
- `KeySpec::parse` becomes a two-line delegation:
  `ConfigValue::parse(&self.kind, self.name, raw)`, returning
  `Result<ConfigValue>`.

## Task 3 — the read seam

`render.rs`:

- `UserConfig::value_of(&self, spec: &KeySpec) -> (ConfigValue, Origin)`.
  Wrap each arm's current expression: `update.check` →
  `ConfigValue::Bool(self.update_check())`, `update.check_interval_hours` and
  `context.ceiling_tokens` → `ConfigValue::Number(...)`, `terminal.backend` →
  `ConfigValue::Text(self.terminal_backend().to_string())`.
- `to_toml_string` (`render.rs:58-68`): route every interpolation through
  `to_toml_literal()` and drop the hand-written `\"{}\"` quoting around
  `terminal.backend` — `to_toml_literal` supplies the quotes now. **You have to
  construct the `ConfigValue` first.** The interpolations today are resolved
  getters, not values: `self.context_ceiling_tokens()` is a `u32`,
  `self.update_check()` a `bool`, `self.terminal_backend()` a
  `SessionBackendKind`, so `to_toml_literal()` cannot be called on any of them.
  Either wrap each —
  `ConfigValue::Number(self.context_ceiling_tokens()).to_toml_literal()`,
  `ConfigValue::Text(self.terminal_backend().to_string()).to_toml_literal()` —
  or, simpler, route the whole function through the seam it now shares and take
  `self.value_of(spec).0.to_toml_literal()` per key. The rendered output must be
  byte-identical to today's; `tests.rs` pins it.

`pressure.rs` and `models.rs`: `pressure_value_of` / `models_value_of` return
`Option<(ConfigValue, Origin)>`, each arm wrapping its `&str` getter in
`ConfigValue::Text(...)`. `pressure_toml` (`pressure.rs:158-168`) and
`models_toml` (`models.rs:135`) move to `to_toml_literal()` the same way, and
they need the same wrapping: their getters return `&str` and their format
strings hand-quote every value with `\"{}\"`. All three helpers carry that
hand-quoting, not only `to_toml_string`. The `Option<String>`
FIELDS on `PressureSection`/`ModelsSection` stay as they are — they are the
parsed file state, not the rendered value, and their `&str` getters
(`pressure_claude_model` and friends) are public API with callers outside this
module. Do not change those getters.

`parse.rs`: rename `get_u32`'s uses to match the `Number` kind if you touch
them, but the function itself is fine as is. **Leave `get_backend` alone** —
`terminal.backend` parses into a real `SessionBackendKind` and
`UserConfig::terminal_backend_set()` hands that typed value to
`crate::fs::work_dir`'s fallback tier. Collapsing it into `get_enum` would
change a public signature outside your territory.

`redirect.rs`: **no change.** It falls inside your glob, so it is worth saying
rather than leaving you to decide: it is a `#[cfg(test)]` thread-local
`Option<PathBuf>` redirect with a Drop guard, and it holds no value type and no
`value_of` or `ConfigValue` reference.

## Task 4 — the write path

`write.rs`:

- `pub fn set(spec: &KeySpec, value: ConfigValue) -> Result<(ConfigValue, ConfigValue)>`
- `pub(crate) fn set_in(path: &Path, spec: &KeySpec, value: ConfigValue) -> Result<(ConfigValue, ConfigValue)>`
- `pub fn unset(spec: &KeySpec) -> Result<(ConfigValue, ConfigValue)>` and
  `unset_in` likewise.
- `locked_edit` returns `(ConfigValue, ConfigValue)`; its `resolved()` helper
  (`write.rs:118-120`) returns `ConfigValue`.
- Inside `set_in`, insert `value.to_toml_edit()` into the table — the
  `toml_edit::Item::Value(...)` call is otherwise unchanged.

Everything about the locking discipline stays exactly as it is: the before/after
pair is still captured inside the one `locked_update` hold, for the reason the
existing doc comment gives.

## Task 5 — the CLI

`commands/config/mod.rs`:

- `print_key` (`:62-67`): `value` is now a `ConfigValue`; `format!("{value}\n")`
  still works through `Display`. No behaviour change.
- `set_key` (`:76-81`): unchanged apart from types flowing through.
- `list` (`:85-103`): the row tuple becomes `(&str, String, String)` still, but
  build the value column with `value.to_string()`. The `value_width` alignment
  at `:95` measures the RENDERED string, so compute it after conversion —
  `ConfigValue` has no `len()`.
- `print_resolved`: unchanged.

`loom config`'s stdout is byte-identical before and after this task. That is the
acceptance bar; `commands/config/tests.rs` already pins several of these strings.

## Task 6 — tests

`loom/src/user_config/tests.rs` is 357 lines and the file ceiling is 400. Add a
new submodule `loom/src/user_config/tests/value.rs` (the directory already
exists — `tests/persistence.rs` lives there) and declare it from `tests.rs`
alongside the existing `mod persistence;`.

In `tests.rs`, four existing tests must change. The list is complete — do not
treat the ones beyond the first as surprises to report:

- `keys_are_typed_as_documented` (`:82-93`) — the `U32` → `Number` rename.
- `each_key_parses_a_valid_value` (`:12-46`) — calls `.as_bool()`,
  `.as_integer()` and `.as_str()` on `spec.parse(...)`. Those are
  `toml_edit::Value` methods and **`ConfigValue` has no accessors at all**, by
  design. Pattern-match on the variant instead. This is the only place in the
  tree that consumes `parse`'s return structurally.
- `to_toml_string_renders_every_key_resolved` (`:308-313`) — matches `key.kind`
  exhaustively (`ValueKind::Bool | ValueKind::U32 => ...`,
  `ValueKind::Enum(_) => ...`). The rename breaks the first arm and the new
  `String` variant makes the match non-exhaustive.
- `value_of_has_an_arm_for_every_registered_key` (`:322-357`) — 18
  `assert_eq!(value, "…")` comparisons against `&str`. `ConfigValue` has no
  `PartialEq<&str>`, so all 18 need `.to_string()`, `:338`'s
  `DEFAULT_CONTEXT_CEILING_TOKENS.to_string()` included.

Plus any other test asserting on `value_of`'s `String`.

In the new `tests/value.rs`, cover:

- Round trip per kind: `parse` → `to_toml_edit` → back through
  `from_toml_value` → equal value.
- `Display` output per kind, pinned to the exact strings `value_of` produced
  before this change (`true`, `24`, `native`, `opus`).
- `to_toml_literal` per kind, including a `Text` containing a `"` to prove the
  escaping comes from `toml_edit`.
- `checked` accepts the matching variant and rejects each mismatch, with the
  message equal to what `parse` produces for the same kind.
- `from_toml_value` rejects a wrong TOML type with a message naming the key.
- **`ValueKind::String` explicitly.** No registry key uses it, so construct a
  `KeySpec` value in the test with `kind: ValueKind::String` and drive `parse`,
  `from_toml_value`, `checked`, `Display` and `to_toml_literal` through it.
  Include a value that is not a valid bool or integer (e.g. `"my project"`) to
  prove the arm is genuinely free text and is not falling through to another
  arm. Without this the variant is defined but never executed. Name this test
  `the_string_kind_accepts_free_text`, verbatim, in module
  `user_config::tests::value` (declared with `mod value;` from `tests.rs`): the
  stage's acceptance runs exactly that test by full path with `--exact` and
  requires `1 passed`, so a rename, or a file left undeclared, turns the gate
  red.

`tests/persistence.rs` should need only signature updates — the on-disk TOML is
unchanged for every existing key.

## Acceptance for your slice

Run at most ONE scoped check, once:

```text
cargo test --manifest-path loom/Cargo.toml --lib user_config::
```

Observed at HEAD: 25 passed. Do not run the full suite, clippy, fmt, or the web
gate — the main agent does that.

Skip even this one if the build is cold. sccache is disabled for this stage, so
if yours is the first cargo invocation in the worktree it compiles all 391 lock
entries from scratch and can outlast the Bash tool's 600000 ms ceiling, where a
kill reads exactly like a build failure. Say in your report that you skipped it
and why; the main agent compiles and tests.

Name the new module's test path in your report: the integration-verify stage
runs `cargo test --lib user_config::tests::value::` as a wiring test, so
`tests/value.rs` must be reachable at exactly that path. A filter matching zero
tests exits 0, so a wrong path fails silently.

## Report

Files changed, the final public surface of `value.rs` if it differs in any way
from the spec above (W2 and W3 are compiling against the spec, so any deviation
must be reported loudly), assumptions made, anything unresolved.
