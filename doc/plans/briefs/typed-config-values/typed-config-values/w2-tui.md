# W2 — Config TUI: per-kind editing

**Tier:** codex `gpt-5.6-terra`, `--effort xhigh`

**Files you own (write):**

- `loom/src/commands/config/tui/mod.rs`
- `loom/src/commands/config/tui/state.rs`
- `loom/src/commands/config/tui/render.rs`
- `loom/src/commands/config/tui/tests.rs` (gains one `mod cycling;` line)
- `loom/src/commands/config/tui/tests/cycling.rs` (new file, unconditional)

**Read-only:** `loom/src/user_config/value.rs`, `loom/src/user_config/keys.rs`.

**Touch nothing else.** `loom/src/commands/config/mod.rs` and its `tests.rs`
belong to W1; `loom/src/user_config/**` belongs to W1.

**Do not run `git` at all.** No `git add`, no `git commit`, no `git status`. The
orchestrator stages and commits.

## Codex units

You are forwarded as three units, in this order, one forward each. A unit must
be completable from this brief alone inside the wrapper's 540 s deadline; the
next forward starts against the tree the previous one left. An exit 124 means
the unit was too large — re-split the remainder into smaller interface-pinned
units rather than re-forwarding the same one.

| Unit | Files written | Steps |
| --- | --- | --- |
| `w2a-state` | `tui/state.rs` | Task 1: retype `ConfigRow`/`PendingValue`; add `displayed()` and retype `displayed_value()`; add `cycle()`; Task 2's `opens_editor()` and `activate()` |
| `w2b-keys-render` | `tui/mod.rs`, `tui/render.rs` | Task 2's `dispatch_key`/`dispatch_edit_key` extraction and bindings; Task 3's kind-aware value cell; Task 3's footer line |
| `w2c-tests` | `tui/tests.rs`, `tui/tests/cycling.rs` | declare `mod cycling;`; the five state cases; the two dispatch cases; the render case |

The tasks below stay the detailed reference; the table only says which forward
does which.

`w2b` and `w2c` compile against what `w2a` wrote, and your source-graph lookups
answer from the published base layer rather than from your own edits, so the
`state.rs` surface those units need is:

```rust
// loom/src/commands/config/tui/state.rs, after w2a
pub(super) struct ConfigRow { /* spec, value: ConfigValue, origin, pending */ }

impl ConfigRow {
    pub(super) fn spec(&self) -> &'static KeySpec;
    pub(super) fn displayed(&self) -> &ConfigValue;   // new in w2a
    pub(super) fn displayed_value(&self) -> String;   // was -> &str
    pub(super) fn origin(&self) -> Origin;
    pub(super) fn is_modified(&self) -> bool;
}

/// new in w2a: true only for `Number` and `String`
pub(super) fn opens_editor(kind: &ValueKind) -> bool;

impl ConfigState {
    pub(super) fn cycle(&mut self, delta: i32);       // new in w2a
    pub(super) fn activate(&mut self);                // new in w2a: Enter
    // unchanged: rows, selected, selected_row, is_editing, edit_buffer,
    // status, status_is_error, move_up, move_down, begin_edit, append_char,
    // backspace, cancel_edit, commit_edit, save
}
```

## The problem you are fixing

`terminal.backend` accepts exactly `native` or `tmux`. The registry says so —
`keys.rs` declares it `ValueKind::Enum(&["native", "tmux"])`. The TUI ignores
that entirely: `state.rs:146-187` has ONE free-text inline editor used for every
key, so switching backends means opening the editor, deleting the word `native`
character by character, and typing `tmux`, with no indication of what is legal
until Enter is rejected. `render.rs:80-114` renders one plain text cell per row.

Meanwhile the web dashboard already dispatches three distinct widgets off the
same registry kind (`web/src/components/settings-control.tsx:34-43`). This
worker brings the TUI to parity.

## The contract W1 is writing (quote, not lookup)

Your source-graph lookups answer from the published base layer and **cannot see
W1's edits**, so the relevant surface is reproduced here. Compile against this.

```rust
// loom/src/user_config/keys.rs
pub enum ValueKind {
    Bool,
    Number,                        // renamed from U32
    Enum(&'static [&'static str]),
    String,                        // new: free text
}

pub struct KeySpec {
    pub name: &'static str,
    pub section: &'static str,
    pub field: &'static str,
    pub kind: ValueKind,
    pub help: &'static str,
}

impl KeySpec {
    pub fn parse(&self, raw: &str) -> anyhow::Result<ConfigValue>;   // was toml_edit::Value
}

// loom/src/user_config/value.rs  (re-exported as crate::user_config::ConfigValue)
pub enum ConfigValue { Bool(bool), Number(u32), Text(String) }

impl ConfigValue {
    pub fn parse(kind: &ValueKind, name: &str, raw: &str) -> anyhow::Result<Self>;
    pub fn to_toml_edit(&self) -> toml_edit::Value;
}
impl std::fmt::Display for ConfigValue;   // Bool -> true/false, Number -> 24, Text -> opus (unquoted)

// loom/src/user_config/mod.rs
impl UserConfig {
    pub fn value_of(&self, spec: &KeySpec) -> (ConfigValue, Origin);   // was (String, Origin)
}
pub fn set(spec: &KeySpec, value: ConfigValue) -> anyhow::Result<(ConfigValue, ConfigValue)>;
```

## Task 1 — `state.rs`

Current shape (read it; these line numbers are advisory and will have drifted):

- `ConfigRow { spec, value: String, origin, pending: Option<PendingValue> }` at
  `:16-25`
- `PendingValue { raw: String, value: toml_edit::Value }` at `:28-33`
- `displayed_value(&self) -> &str` at `:53-57`
- `commit_edit` at `:174-187` — validates through `spec.parse` and stages
- `save` at `:190-...` — writes each pending row through `crate::user_config::set`

Changes:

- `ConfigRow.value: ConfigValue`.
- `PendingValue { raw: String, value: ConfigValue }`. Keep both fields: `raw`
  is what the operator sees while a text edit is open, `value` is what `save`
  writes. For a value produced by cycling rather than typing, set
  `raw = value.to_string()`.
- `displayed_value(&self) -> String` (no longer `&str`, since a `ConfigValue`
  renders on demand). Update `render.rs`'s call site accordingly.
- Also add `pub(super) fn displayed(&self) -> &ConfigValue` beside it: the
  pending value when one is staged, else the row's disk value. `render.rs`
  needs the typed value, not its rendering — without this accessor the only way
  to render a bool as a checkbox is `displayed_value() == "true"`, which is the
  exact string re-derivation this plan exists to delete.
- `save` (`:190`) keeps its shape. Its pending tuple becomes
  `Vec<(usize, &'static KeySpec, ConfigValue)>` — the annotation is explicit at
  `:191` — and the `crate::user_config::set` return stays discarded:
  `refresh_after_save` (`:218-255`) reloads a strict snapshot and reassigns
  every row from a fresh `value_of` at `:233-237`, which now yields a
  `ConfigValue`. Do not restructure that; the doc comment at `:217` says why it
  reloads rather than patching rows in place.

New behaviour on `ConfigState`:

```rust
/// Step the selected row's value without opening the text editor.
///
/// `delta` is +1 for forward, -1 for backward. A Bool toggles (either
/// direction). An Enum steps through its variants and WRAPS at both ends. A
/// Number or String has no ordered vocabulary to step through, so this sets a
/// status line pointing at the editor and changes nothing.
pub(super) fn cycle(&mut self, delta: i32)
```

Implementation notes:

- Read the CURRENT value to step from: the pending value if one is staged,
  otherwise the row's disk value. Cycling twice in a row must advance twice,
  not bounce off the disk value.
- For `ValueKind::Enum(variants)`: find the current text's index in `variants`.
  A value not in the list (possible if the file holds a variant the registry no
  longer lists) starts from index 0 rather than panicking. Wrap with
  `rem_euclid` over `variants.len() as i32`, not `%` — a `-1` from index 0 must
  land on the last variant.
- For `ValueKind::Bool`: negate. `delta` is ignored.
- For `ValueKind::Number` / `ValueKind::String`: leave the row untouched and
  set an error-styled status,
  `format!("{} has no variants to cycle; press Enter to edit.", spec.name)`.
  Not "is free text" — that arm also fires for `context.ceiling_tokens`, and a
  context ceiling is not free text.
- Every successful cycle stages a `PendingValue` exactly the way `commit_edit`
  does, sets the same `"{} staged; press s to save."` status, and leaves the row
  marked modified so `s` writes it.
- Build the staged value directly (`ConfigValue::Bool(..)` /
  `ConfigValue::Text(..)`), not by round-tripping through `spec.parse` — the
  variant came from the registry's own vocabulary, so re-parsing it proves
  nothing and would turn an unlisted disk value into an error the operator
  cannot escape.

## Task 2 — `mod.rs` key handling

`handle_key` (`:116-144`) currently maps only `q`/`Esc`, `Up`/`k`, `Down`/`j`,
`Enter`, `s`. Add, in the non-editing branch only:

- `KeyCode::Left` | `KeyCode::Char('h')` → `self.state.cycle(-1)`
- `KeyCode::Right` | `KeyCode::Char('l')` → `self.state.cycle(1)`
- `KeyCode::Char(' ')` → `self.state.cycle(1)`

- `KeyCode::Enter` → `self.state.activate()` (was `begin_edit()`).

**`Enter` opens the text editor only for `Number` and `String` keys.** On a
`Bool` it toggles and on an `Enum` it steps forward, exactly like `→`. This is
the plan's goal — only free-form kinds get a text field — and it removes the
path by which an operator could type an invalid variant. The decision lives in
`state.rs`, not in the key handler, so a headless test can reach it:

```rust
/// Whether `kind` is edited as text. `Bool` and `Enum` have a closed
/// vocabulary and are stepped with the cycle keys instead.
pub(super) fn opens_editor(kind: &ValueKind) -> bool {
    matches!(kind, ValueKind::Number | ValueKind::String)
}

impl ConfigState {
    /// The Enter key on the selected row: open the text editor for a free-form
    /// kind, otherwise step the value forward (`cycle(1)`).
    pub(super) fn activate(&mut self)
}
```

**Make the key dispatch testable.** `ConfigTui` owns a live terminal, so no test
can build one. Move the bodies of `ConfigTui::handle_key` (`:116-144`) and
`handle_edit_key` (`:147-156`) into two private free functions in `mod.rs`,
`fn dispatch_key(state: &mut ConfigState, key: KeyEvent) -> bool` and
`fn dispatch_edit_key(state: &mut ConfigState, code: KeyCode) -> bool`, with
unchanged behaviour apart from the bindings above, and reduce the two methods to
one-line delegations (`dispatch_key(&mut self.state, key)`). A private item of
`tui/mod.rs` is visible to its descendant `tui::tests::cycling`, so the tests
call `super::super::dispatch_key` with no visibility change. Keep
`dispatch_key` under 50 lines; it is about 40 with the new arms.

Leave the editing branch's behaviour alone: while the editor is open, `h`, `l`
and space are literal characters the operator is typing.

Ctrl+C handling (`:117-119`) stays first in `dispatch_key`, unchanged.

## Task 3 — `render.rs`

`render_row` (`:80-114`) builds the value cell. Make the cell kind-aware.

Branch on `row.spec().kind` and read the value through `row.displayed()` for
the `Bool` and `Enum` cells — that accessor hands you the `ConfigValue`, so you
match on the variant instead of comparing rendered text. Keep
`row.displayed_value()` for the `Number`/`String` text cell, where a staged
`pending.raw` deliberately differs from `pending.value` (`00042` stays visible
while the value is `Number(42)`).

Render the row's displayed value as:

- `ValueKind::Bool` → `[x] on` when true, `[ ] off` when false. Write each as
  one string literal (`"[x] on"`, `"[ ] off"`); a wiring check greps
  `"[x] on"` in `render.rs`.
- `ValueKind::Enum(_)` → `‹ native ›` when the row is selected and no text edit
  is open, plain `native` otherwise. The guillemets signal "this steps".
- `ValueKind::Number` / `ValueKind::String` → the plain value, as today.

While a text edit is open on the selected row, keep the existing
`format!("{buffer}▏")` cursor rendering for every kind — an open editor looks
the same whatever the kind is.

Measure nothing with `chars().count()`. The value column is a fixed
`Constraint::Length(24)` and ratatui does its own truncation, so you are not
padding by hand; just do not introduce a manual pad.

`render_row` is 35 lines today against a hard 50-line ceiling
(`loom/tests/maintainability/scanner.rs:6-7`). Four kinds plus the open-editor
branch will not fit, so lift the cell construction into a private
`value_cell(row: &ConfigRow, selected: bool, editing: bool) -> String` (or a
`Span`) rather than growing the function.

`render_footer` (`:130-142`) gains the new binding. Keep the existing style —
`Span::styled(key, Theme::header())` then `Span::raw(" label  ")`:

```text
↑↓/k/j move  ←/→/space cycle  Enter edit or cycle  s save  Esc/q quit  * pending
```

## Task 4 — tests

Every new case goes in a NEW file, `loom/src/commands/config/tui/tests/cycling.rs`,
declared by a `mod cycling;` line at the TOP of
`loom/src/commands/config/tui/tests.rs`. This is unconditional — not "if
`tests.rs` would pass 400 lines". `tests.rs` is 139 lines with 6 tests today and
keeps all of them; `tui/mod.rs:14-15`'s `#[cfg(test)] mod tests;` declaration is
unchanged.

The file names and the eight function names below are pinned by the stage's
`after_stage` greps and `wiring_tests` filters. Use them verbatim; a rename
turns a gate red.

Cover, against the real `KEYS` registry:

- `cycles_an_enum_forward_and_wraps` — cycling `terminal.backend` forward from
  `native` stages `tmux`; forward again wraps to `native`. Assert
  `displayed_value()` reflects the staged value rather than the disk value at
  each step, so the accessor is covered here and needs no separate case.
- `cycles_an_enum_backward_and_wraps` — backward from `native` stages `tmux`.
  This is the `rem_euclid` case; a `%` implementation fails it.
- `toggles_a_bool_with_space` — space on `update.check` toggles the bool and
  stages it.
- `refuses_to_cycle_a_number_key` — cycling `context.ceiling_tokens` sets an
  error status and changes nothing. Assert all three: `is_modified()` stays
  false, `displayed_value()` is byte-identical to the value captured before the
  cycle, and the status text is the refusal message. Asserting only
  `is_modified() == false` asserts the row's initial state and would pass
  against a no-op `cycle`.
- `saves_a_cycled_enum_to_disk` — a cycle followed by a save writes the cycled
  value. Drive the real `ConfigState::save` seam the existing tests already use:
  `tests.rs:9-15` builds a `tempdir`, joins `config.toml`, calls
  `redirect_user_config(path)` and holds the guard, and
  `failed_write_reports_the_key_and_leaves_the_edit_staged` (`:117-139`) reads
  and writes that path directly. Copy that setup and assert the file holds
  `backend = "tmux"`.
- `enter_edits_only_the_free_form_kinds` — drive `dispatch_key` with
  `KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)`. On `update.check` the
  bool flips, the row is modified, and `is_editing()` is false. On
  `terminal.backend` the row stages `tmux` and `is_editing()` is false. On
  `context.ceiling_tokens` `is_editing()` is true and nothing is staged. Then
  assert `opens_editor` directly for all four kinds, including
  `ValueKind::String` (no registry key has it, so this is its only coverage):
  true for `Number` and `String`, false for `Bool` and `Enum(&["a"])`.
- `cycle_keys_dispatch_through_the_key_handler` — drive `dispatch_key` with
  `Left`, `Right`, `Char('h')`, `Char('l')` and `Char(' ')` on
  `terminal.backend` / `update.check` and assert each staged value, then open
  the editor on `context.ceiling_tokens` with Enter and assert `Char('h')`,
  `Char('l')` and `Char(' ')` land in `edit_buffer()` instead of cycling.
  Calling `cycle` directly proves nothing about the key map; this case does.
- `bool_cell_renders_checkbox_and_enum_cell_renders_guillemets` — the render
  test. Nothing else in this stage proves what text a cell holds: the compiler
  does not care, and the `KeyCode::Left` grep proves only that the key is bound.
  Assert, off a rendered frame, that the `update.check` row shows `[x] on` when
  true and `[ ] off` when false, that a selected `terminal.backend` row shows
  `‹ native ›`, that a `Number` row shows its plain value, and that the footer
  advertises the cycle binding.

## Size limits

A function over 50 lines is a hard gate, not a style note:
`loom/tests/maintainability/scanner.rs:6-7` sets `FILE_LINE_LIMIT = 400` and
`FUNCTION_LINE_LIMIT = 50`, measured against an exact ledger in
`loom/maintainability-baseline.txt`, and no file in your territory has an entry
there today — so any breach is a new violation that fails the stage. Split
rather than grow. The two bodies under pressure here are `render_row` (35 lines
today) and the new `cycle` (four kinds, the `rem_euclid` wrap, staging and the
status line).

## Acceptance for your slice

Run at most ONE scoped check, once:

```text
cargo test --manifest-path loom/Cargo.toml --lib commands::config::tui::
```

Do not run the full suite, clippy, fmt, or anything under `web/`. The main agent
compiles, tests, lints and fixes.

## Report

Files changed, the key bindings as implemented, anything about the cycling
semantics you had to decide that this brief did not specify, anything
unresolved.
