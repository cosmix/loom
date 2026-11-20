# G2: the dispute field, end to end outside adjudication, and dispute bookkeeping

Stage `completion-gates`, wave 1, tier sonnet. Read `../common.md` first. G3 (wave 2) owns
everything under `loom/src/orchestrator/adjudication/` and `loom/src/verify/goal_backward/mod.rs`;
do not touch those files, even where they stop compiling after your change. Run no check: the
crate compiles again only after G3.

## Why

`loom stage dispute-criteria` can dispute only an acceptance criterion. A wrong wiring check or
wiring test has no route, so an agent either weakens its work or stops. The settled design:
`--field acceptance|wiring|wiring-tests` (default `acceptance`); `--criterion-index` indexes
into that field's list; the criterion budget (`dispute_count`) is shared by all three. Two
bookkeeping gaps close with it: a refused filing leaves an open `request.md`, and
`loom stage reset` closes no open dispute.

## 1. Foundation, first: the type (`loom/src/models/dispute.rs`)

- Add, above `DisputeKind`:

      /// Which list of a stage a criterion dispute contests.
      #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
      #[serde(rename_all = "kebab-case")]
      pub enum CriterionField {
          /// `acceptance`: the stage's acceptance criteria.
          #[default]
          Acceptance,
          /// `wiring`: the stage's wiring checks.
          Wiring,
          /// `wiring_tests`: the stage's wiring tests.
          WiringTests,
      }

  with three methods: `pub fn as_str(self) -> &'static str` (`"acceptance"`, `"wiring"`,
  `"wiring-tests"`, the CLI and serde spelling); `pub fn gap_label(self) -> &'static str`
  (`"criterion"`, `"wiring"`, `"wiring_tests"`, the label `loom stage complete` prints as
  `[<label> n]`); `pub fn amendment_field(self) -> crate::plan::amendment::AmendmentField`.
- `DisputeKind::Criterion { criterion_index: usize, field: CriterionField }`. The field is
  required: no `#[serde(default)]` (the project carries no backward-compatibility code). Its
  doc: "One entry of `field`'s list: `loom stage dispute-criteria`."
- `impl DisputeKind { pub fn criterion(field: CriterionField, criterion_index: usize) -> Self }`.
  Every construction site uses it, which keeps literals on one line.
- `models/dispute_tests.rs`: migrate the two `request(DisputeKind::Criterion { .. })` lines to
  the constructor; add `a_criterion_request_round_trips_its_field` (a `WiringTests` kind
  survives YAML and serializes `field: wiring-tests`).
- `models/stage/dispute_budgets.rs`: the criterion budget is shared by every field. `filed`,
  `exhausted` and `spend` already match `DisputeKind::Criterion { .. }`; only the test
  literals move to the constructor. Add `every_criterion_field_spends_the_criterion_budget`.

## 2. CLI

- `cli/types_stage_disputes.rs`: a clap mirror, the pattern `cli/types_stage_amend.rs` uses for
  `AmendField`:

      /// Which list `loom stage dispute-criteria` indexes.
      #[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
      pub enum DisputeField { Acceptance, Wiring, WiringTests }

  with `pub fn to_field(self) -> CriterionField`. clap renders the values `acceptance`,
  `wiring`, `wiring-tests`.
- `cli/types_stage.rs` `StageCommands::DisputeCriteria` (about line 214): add

      /// Which list --criterion-index indexes: `acceptance` (the default), `wiring` or
      /// `wiring-tests`, as `loom stage complete` labels a failure `[criterion n]`,
      /// `[wiring n]` or `[wiring_tests n]`.
      #[arg(long, value_enum, default_value = "acceptance")]
      field: DisputeField,

  and reword `criterion_index`'s doc to "Index (0-based) of the disputed entry in --field's
  list". Reword the command's doc so it names wiring checks and wiring tests.
- `cli/dispatch_stage.rs::dispatch_stage_criteria` passes `field.to_field()`.
- `commands/stage/dispute_criteria.rs`: `dispute_criteria(stage_id, field, criterion_index,
  reason, evidence_commit, failure_output_path)` and `dispute_criteria_with_mode` gain `field`
  after `stage_id`; update `dispute_criteria_tests.rs` call sites and literals, and add
  `relay_mode_dispute_carries_its_field` (the relayed payload holds `"field": "wiring"`).

## 3. Transport and wire

- `commands/stage/dispute_transport.rs`: `Dispute::criterion(stage_id, field, criterion_index,
  reason, evidence_commit, failure_output)` builds `DisputeKind::criterion(field, index)`;
  `queued()` and `rpc()` carry `field`; `wording()` names the subject `criterion <n>` for
  acceptance, `wiring <n>` and `wiring-tests <n>` for the others. Leave the relay `end_turn`
  argument alone (stage `stage-exits` changes it).
- `daemon/protocol.rs` `Request::DisputeCriteria`: add `field: CriterionField` after
  `session_id`. `daemon/protocol_debug.rs::debug_dispute` prints it after `criterion_index`
  (its pattern ends in `..`, so leaving `field` out still compiles: add an inline
  `#[cfg(test)] mod tests` to `protocol_debug.rs` with `dispute_criteria_debug_names_the_field`,
  asserting the `Debug` output of a `WiringTests` request contains `field`). Do not add tests
  to `daemon/wire_tests.rs`: it is 372 lines, and stage `stage-exits` adds one there later;
  only its two `Request::DisputeCriteria` literals gain the field.
- `fs/stage_request/types.rs` `StageRequest::Dispute`: add `field: CriterionField` (required)
  and update the variant's doc to `--field <f> --criterion-index N`.
- `daemon/server/stage_control.rs`: `serve_dispute_criteria` and the `Request::DisputeCriteria`
  arm of `serve_stage_control` pass `field` through.
- `fs/stage_request/apply.rs` (about line 61) and `orchestrator/core/inbox_drain/apply.rs`
  (about line 114): destructure `field` and pass it to `handle_dispute_criteria`.
- `daemon/server/self_service.rs` needs no edit: its patterns end in `..`.
- Test literals and JSON gain the field (`CriterionField::Acceptance`, or `"field":
  "acceptance"` in JSON): `daemon/wire_tests.rs`, `daemon/server/self_service_tests.rs`,
  `daemon/server/tests/self_service_client.rs`, `fs/stage_request/tests/{apply,spool}.rs`,
  `orchestrator/core/inbox_drain/test_support.rs` (the struct literal and the JSON at about
  line 235), `orchestrator/core/verdict_apply_tests.rs`, `orchestrator/signals/adjudication.rs`
  (its test at about line 86), and `relay/payload.rs` (the test JSON at about line 118).
- `tests/adjudication_e2e.rs` is ledgered at exactly 525 lines. Its one literal becomes
  `kind: loom::models::dispute::DisputeKind::criterion(Default::default(), 0),`, which stays on
  one line. Nothing else in that file changes.

## 4. The daemon handler and bookkeeping

`daemon/server/dispute.rs::handle_dispute_criteria` becomes

    pub fn handle_dispute_criteria(
        work_dir: &Path,
        stage_id: &str,
        field: CriterionField,
        criterion_index: usize,
        reason: String,
        evidence_commit: Option<String>,
        failure_output: Option<String>,
    ) -> Result<Response>

(`loom::daemon::handle_dispute_criteria` re-exports it; the contracts call it). In order:

1. `validate_id` as today.
2. Lock, load the stage.
3. Range check against the field's list: `acceptance.len()`, `wiring.len()` or
   `wiring_tests.len()`. Refuse with `Response::Error` whose message is
   `criterion_index <i> out of range for --field <field> (stage has <len> entries)` (keep the
   words "out of range"; a test matches them).
4. Budget: unchanged (escalate and refuse when `dispute_budget_exhausted`).
5. NEW: check the transition before anything is written. Clone the loaded stage and call
   `try_request_adjudication(Some(reason.clone()))` on the clone; on `Err` refuse with
   `Response::Error { message: format!("cannot dispute stage '{stage_id}': {error:#}") }`.
   Nothing is written and `dispute_count` does not move.
6. Truncate `failure_output`, build the `DisputeRequest` with `DisputeKind::criterion(field,
   criterion_index)`, `write_request`, then the `update_stage` transition as today.

The function is ledgered at 91 lines (`function src/daemon/server/dispute.rs
handle_dispute_criteria 91` in `loom/maintainability-baseline.txt`). Refactor it under 50 lines
by extracting helpers (the range check, the exhausted-budget refusal, the pre-write transition
check, the record build), then REMOVE that ledger line. That is the stage's one expected
integrity event. Update the module doc comment's numbered steps to the new order.

`dispute.rs` is 384 lines and its tests are inline (`mod tests` at about line 146). Adding
`CriterionField::Acceptance,` pushes nine of the ten `handle_dispute_criteria(` test calls
past 100 columns, and rustfmt splits each over about nine lines, so the file would pass the
400-line limit. FIRST move the inline `mod tests { .. }` body verbatim into a new
`src/daemon/server/dispute_tests.rs`, and declare it in `dispute.rs` as
`#[cfg(test)] #[path = "dispute_tests.rs"] mod tests;` (the pattern at the end of
`dispute_kinds.rs`). The test path stays `daemon::server::dispute::tests::*`. `dispute.rs` is
a production file to the integrity gate (it matches no test-file glob), so moving its tests
raises no event; the new `_tests.rs` file only adds to the assertion totals. Then make every
test edit in `dispute_tests.rs`: migrate the existing calls (add `CriterionField::Acceptance`);
add `wiring_index_is_checked_against_the_wiring_list`,
`wiring_tests_index_is_checked_against_the_wiring_tests_list`, and
`a_refused_transition_writes_no_request_and_spends_no_budget` (stage `Completed`).

`daemon/server/dispute_kinds.rs::handle_file_dispute` has the same gap (it writes
`request.md`, then `update_stage` may refuse the transition): check the transition on a clone
of the loaded stage right after `confirm` and the budget check, and refuse with
`refused(format!("cannot dispute stage '{}': {error:#}", stage.id))` before `write_request`.
The function is 43 lines: keep the new check to one `if let Err(error) = ..` block so it stays
under 50.
Add `a_refused_file_dispute_writes_no_request` to `dispute_kinds_tests.rs`. In that file's
existing key-set test (about line 176), add `"field",` to the `expected` array between
`"failure_output"` and `"fix_attempts_at_dispute"`; the `assert_eq!` lines stay unchanged.

## 5. Reset closes open disputes

`commands/stage/state/loop_recovery/mod.rs::reset_with`: after the `update_stage(...
apply_reset ...)` call succeeds, call
`crate::orchestrator::adjudication::close_open_disputes(work_dir, stage_id)`. It runs after the
stage lock is released, which is the order the filing path needs (dispute lock, then stage
lock; `orchestrator/adjudication/apply.rs:175-180` states it). Say so in a one-line comment.
Test in `commands/stage/state_tests.rs`: `reset_closes_open_disputes` (exact name; acceptance
runs it): a stage with `disputes/<id>/1/` and no `applied.marker` gets a `closed.marker`
after `reset_with`. Assert on the literal file name, e.g.
`work_dir.join("disputes/<id>/1/closed.marker").exists()`: the `CLOSED_MARKER` constant is
private to `orchestrator/adjudication/closed_disputes.rs` (G3 owns that file and updates the
doc comment of `close_open_disputes`, which today says it runs only after an escalation).

## 6. Dynamic completion for `--field`

`completions/dynamic/mod.rs::complete_flag_value` ignores its `_cmd_path` today and has no
`--field` arm, so `loom stage dispute-criteria s1 --field <TAB>` falls through to stage ids.
Add `"--field" => Ok(Some(complete_flag_choices(cmd_path, "field", prefix)?))` and, in
`completions/dynamic/commands.rs`, `pub fn complete_flag_choices(command_path: &[&str],
arg_id: &str, prefix: &str) -> Result<Vec<String>>`, which finds the command with the
existing `command_at_path(&Cli::command(), command_path)` and returns the names of that
argument's `get_possible_values()` that start with `prefix`. It serves `loom stage amend
--field` too. Test in `completions/dynamic/tests/tests_stage.rs`:
`field_values_complete_for_dispute_criteria` (the choices for `wi` are `wiring` and
`wiring-tests`). `complete_after_subcommand` (ledgered, 56 lines) is not touched.

## Contracts your code must satisfy

`wiring-dispute-indexes-the-wiring-list` and `refused-dispute-writes-no-request` (scenarios in
the plan's YAML).
