# Typed Config Values: the `ConfigValue` Read-Path Seam

> ConfigValue typed read-path across CLI/TUI/web/TS/React
> web API, TypeScript client, React — instead of collapsing to a string at the read boundary.

## The Seam

Before this, `UserConfig::value_of` (`loom/src/user_config/render.rs:17-43`) returned `(String,
Origin)` for every key regardless of kind, and each downstream surface re-derived the type it was
never sent (regex/`==` string comparisons in the TUI and the TS client). `ConfigValue`
(`loom/src/user_config/value.rs:24-28`) closes that gap:

```rust
#[serde(untagged)]
pub enum ConfigValue { Bool(bool), Number(u32), Text(String) }
```

`#[serde(untagged)]` makes the JSON wire native — `Bool(true)` serializes as `true`, `Number(24)` as
`24`, `Text("opus")` as `"opus"` — and the declaration order matters: untagged deserialization tries
variants in order, so `Bool` must be tried before `Number` or a JSON `true` risks matching the wrong
arm first.

`ValueKind` (`loom/src/user_config/keys.rs:15-24`) is the TOML *shape* a `KeySpec` accepts —
`Bool`, `Number`, `Enum(&'static [&'static str])`, `String` (free text) — one level more specific
than `ConfigValue`'s three variants: an `Enum` and a free `String` both carry as `ConfigValue::Text`,
distinguished only by which `ValueKind` they're checked against.

`value_of` now returns `(ConfigValue, Origin)`, built from `KeySpec::parse` (`keys.rs:192-194`) on
the write path and from `ConfigValue::checked` (see below) on the read path, so both paths share one
typed representation.

## `ConfigValue::checked` — the Mismatch Contract

`checked(self, kind: &ValueKind, name: &str) -> Result<Self>` (`value.rs:154-172`) validates a value
against the kind a caller expects (`Bool`↔`Bool`, `Number`↔`Number`, `Text`↔`Enum`/`String`, `Enum`
additionally requiring the text be a listed variant). **On a mismatch it always errors — never
coerces**, even when the value's `Display` rendering would happen to reparse as `kind` (e.g.
`Text("false")` checked against `Bool` is rejected, not silently accepted as `Bool(false)`). For
`Bool`/`Number`/`Enum` the error reproduces the exact wording `ConfigValue::parse` would have
produced for that `kind` given the value's `Display` text — formatted via `{:?}` (debug-quoted) so a
dashboard client's `"abc"` and a CLI operator's typed `abc` report identically. `String` has no
`parse` failure to match, so `string_error` is its own message. A test asserting reject-on-mismatch
needs a value whose TEXT would still parse for the target kind (e.g. `Text("false")` vs `Bool`) —
one that never reparses proves nothing about coercion.

## Threading Through the Four Downstream Surfaces

- **CLI** (`loom/src/commands/config/mod.rs:65,90`): calls `config.value_of(spec)`, `ConfigValue`
  inferred from the tuple return — a wiring grep for the literal `ConfigValue` in this file will
  never match (see [pinned-literals-ledgers-and-wiring](../mistakes/pinned-literals-ledgers-and-wiring.md)).
- **TUI** (`loom/src/commands/config/tui/`): edits per-kind — arrows/space cycle an `Enum`/`Bool`,
  `Enter` opens a text editor only for `Number`/`String`. `tui/render.rs`'s `value_cell` renders
  `ConfigValue::Text` straight into a ratatui `Cell` with **no control-character stripping** — inert
  today because no `KEYS` entry uses `ValueKind::String` and the TUI only ever edits the operator's
  own file; strip control characters before the first `String`-kind key ships, or accept the risk
  explicitly (see [concerns.md](../concerns.md)).
- **Web API** (`config_api/`): `POST /api/config` carries native JSON — `true`, `800000`, `"opus"` —
  not a wrapped string. `configValueSchema` on the TS side (`web/src/api/config.ts`) stays
  kind-agnostic (`z.union` of boolean/number/string for every entry) rather than refining per
  `kind.type`; the server is the sole validator, and `formatValue` deliberately tolerates a
  mismatch. This means an out-of-range or malformed JSON body (a `Number` field as `-1` or `1.5`)
  fails ALL three untagged variants before any registry validator runs, producing serde's generic
  "did not match any variant" instead of the registry's friendly range message — accepted as a wire
  limitation of `#[serde(untagged)]`, not fixed by this plan.
- **React** (`web/src/components/settings-*.tsx`): `EnumSelect` keeps a code path that synthesizes a
  display option for a current value that is off the registry's variant list, even though the
  `ConfigValue::Enum` invariant makes that path unreachable today — kept deliberately as a guard
  against a future registry dropping a variant (`settings-control.tsx:154`).

## Test Coverage Gap

`ConfigState`'s fields are private to `tui::state`, unreachable from `tui::tests::cycling` as a
sibling module, and no `KEYS` entry has kind `String` — so nothing in the TUI test suite can build a
`ValueKind::String` row and drive `ConfigState::cycle` on it. `String`'s refuse-to-cycle behavior is
covered only indirectly: sharing the `Number` arm in `cycle`, plus a `ConfigTui::opens_editor(&ValueKind::String)`
assertion. Closing this gap needs either a registered `String` key or a `pub(super)` test
constructor for `ConfigState`.

## TS Fixture Coupling

`web/src/components/settings-model.test.ts` is fixture-driven, not inline: it imports
`@/api/fixtures/config.json` and calls `configResponseSchema.parse(fixtureJson)` at module scope, so
every `sectionRows`/`laneState`/`filterSections` test runs off that one real fixture. Any change to
`configKindSchema` in `web/src/api/config.ts` throws at import time unless the fixture is
regenerated in the same change.
