# live-profile S1: the `live:` stage field

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. The frozen
contracts are `tests/live_schema_contracts.rs` (and its harness); never edit them. The stage
description's PUBLIC SURFACE is authoritative. You own only the files listed for S1 in the stage
table.

## The type: new file `src/plan/schema/types_live.rs`

```rust
/// One command run inside loom's live profile (plan `version: 2`, standard and
/// integration-verify stages). Only `loom check --live <stage>` runs it; `loom stage complete`
/// reads the receipt that run wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveEntry {
    /// Shell command run with `sh -c` from the stage's working directory.
    pub command: String,
    /// Seconds the command may run; default 1800.
    #[serde(default = "default_live_timeout_secs")]
    pub timeout_secs: u64,
    /// Hosts the gate serves for this entry; default every host of `credential`.
    #[serde(default = "default_live_hosts")]
    pub hosts: Vec<String>,
    /// Whose credential the gate injects; `anthropic` is the only kind.
    #[serde(default)]
    pub credential: LiveCredential,
    /// Whether `loom stage complete` needs a passing run of this entry for the current content.
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LiveCredential { #[default] Anthropic }
```

Also in that file: `impl LiveCredential { pub fn hosts(self) -> &'static [&'static str] }`
(`Anthropic` returns `crate::live::gate::ANTHROPIC_HOSTS`, the gate's one spelling of the
host), `pub const LIVE_PREPARE_SECS:
u64 = 60;` (run root, gate and HOME seeding before the command's own timeout starts), `pub
const LIVE_TIMEOUT_SECS: RangeInclusive<u64> = 60..=7200;`, `pub const LIVE_MAX_ENTRIES: usize
= 8;`, `impl LiveEntry { pub fn deadline_secs(&self) -> u64 {
self.timeout_secs.saturating_add(LIVE_PREPARE_SECS) } }`, the two default fns, and `pub fn
live_entry_hash(entry: &LiveEntry) -> String` (hex SHA-256 over the field
`b"live-entry-v1"` then `serde_json::to_vec(entry)`, the length-prefixed form
`handoff/completion/identity.rs` `hash_field` uses). The hash is per entry so that amending one
entry leaves the receipt of the others valid. Declare `mod types_live;` in
`src/plan/schema/mod.rs` and `pub use` every public item.

## Carrying it through

- `src/plan/schema/types.rs` `StageDefinition`: add after `reachable`
  `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub live: Vec<LiveEntry>,` with a
  one-line doc comment. `has_any_goal_checks` does not change.
- `src/models/stage/types.rs` `Stage`: the same field after `reachable` ("Live entries. Copied
  from the plan's StageDefinition."). This file has an exact ledger row (`file
  src/models/stage/types.rs 591`): it must still be exactly 591 lines afterwards. Make room by
  condensing the six-line doc comment on `stall_recoveries` to three lines that keep every fact
  (stall recovery bound; persisted so a restarted daemon cannot reset it; owned by
  `event_handler::recover_hung`). Check with `wc -l`. If plans 08-11 changed the file so the
  ledger value differs, keep the ledger value and report it.
- `src/models/stage/defaults.rs` `Stage::default` is `#[rustfmt::skip]` and kept at its
  ledgered 76 lines by pairing empty lists: put `live: Vec::new(),` on the `reachable:` line.
  Function length must not change.
- `src/models/stage/methods.rs` `Stage::from_definition`: add `stage.live =
  definition.live.clone();` after the `reachable` line. Exact ledger row (`file
  src/models/stage/methods.rs 493`): remove one line elsewhere without changing behaviour
  (reflow the three-line doc comment of `from_definition` to two lines) and confirm `wc -l`.
- `src/fs/stage_loading.rs` (the Stage to StageDefinition round trip): add `live:
  stage.live.clone(),`.
- `src/fs/tests_stage_loading_round_trip.rs` (module `fs::stage_loading::tests_round_trip`):
  the exhaustive sample definition gains `live: vec![sample_live_entry()]` (a non-default
  entry: timeout 2400, required true) so the existing round-trip assertion covers the field. No
  assertion line changes.
- `src/handoff/completion/identity.rs` `GatePolicy`: add `live: &'a
  [crate::plan::schema::LiveEntry],` after `reachable` and `live: &stage.live,` in
  `check_definition_hash`.
- `src/host_verify/runner.rs` `job_deadline_secs`: add
  `.saturating_add(stage.live.iter().map(LiveEntry::deadline_secs).fold(0u64, u64::saturating_add))`
  and extend its doc comment ("... and each live entry's timeout plus `LIVE_PREPARE_SECS`, for
  a live run"). `runner.rs` is 387 lines: keep it under 400.

## Validation: new file `src/plan/schema/validation/v2_fields/live.rs`

`pub(super) fn push_live_problems(stage: &StageDefinition, messages: &mut Vec<String>)`, called
from `push_v2_rules` in `v2_fields.rs` for every stage (declare `mod live;` beside the other
child modules). Messages, numbered from 1 (`Live #{n}`), exactly as the PUBLIC SURFACE lists
them. In `push_v1_uses` add `("`live`", stage.live.is_empty())` to the per-stage list, so a v1
plan gets "`live` requires `version: 2`".

## The escape lint over live commands

`src/plan/schema/validation/v2_lints/escape_hatches.rs` (written by test-escape-policy): `check`
also scans each `stage.live[i].command` with `escapes_in(command, true)` (every escape but
`--include-ignored`, which selects the ignored tests a live entry exists to run), label `Live
entry #<i+1>`. Update the module doc's "not scanned" list.

Unit tests in `src/plan/schema/tests/v2_live_tests.rs` (declare it in
`src/plan/schema/tests/mod.rs` beside the other v2 test modules) for every message above and
for the live-command escape scan, mirroring `src/plan/schema/tests/v2_tests.rs`
(`create_valid_metadata_v2`, `make_stage`).

## Traps

- `StageDefinition` literals: only `src/fs/stage_loading.rs` and
  `src/fs/tests_stage_loading_round_trip.rs` list every field; the rest use
  `..Default::default()`. `cargo build --all-targets` names any other.
- `Stage` literals: every one starts from `Stage::default()` except `defaults.rs`.
- Your one check: `cargo test --lib plan::schema:: --no-run`.

## Report

Files changed, the final `wc -l` of `src/models/stage/types.rs` and
`src/models/stage/methods.rs`, and any literal site the compiler named that this brief did not.
