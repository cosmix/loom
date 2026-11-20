# S2: `loom status` shows why a stage agent blocked the stage

Stage `stage-exits`, tier sonnet. Read `../common.md` first. Read the knowledge section
`mistakes/computed-values-and-hidden-couplings.md#StageSummary Is a Wire Type: Status Clients
Recompute Attention From Deserialized Data` before you start (your stage's Knowledge Brief may
quote it).

You own `loom/src/commands/status/data/{mod.rs, collector.rs, sanitize.rs}`,
`loom/src/commands/status/web/{model.rs, model_tests_stages.rs}`,
`loom/src/commands/status/render/{attention_model.rs, attention_model_guidance_tests.rs,
attention_model_tests.rs, attention_tests.rs, graph_tests.rs}`,
`loom/src/commands/status/ui/tui/{ledger/tests.rs, ledger/rows.rs, state_tests.rs}` and
`loom/src/daemon/wire_tests.rs`.

## Why

A BLOCKED stage with no `failure_info` shows only `loom stage retry <id>`
(`render/attention_model.rs::blocked_guidance` → `retry_guidance`, about lines 225-249); the
reason the agent gave with `loom stage block` never reaches the operator. The terminal
dashboard (TUI) computes attention from `StageSummary` frames it DESERIALIZES from the daemon,
so a field that is not serialized is invisible to it. The browser gets attention precomputed
by `web/model.rs`, and its `stageSummarySchema` (`web/src/api/schema.ts`) is `.strict()`, so an
unknown key breaks the page.

## 1. Carry the reason

- `data/mod.rs::StageSummary`: after `merge_resolver_attempts`, add

      /// Why the stage was closed or blocked (`stage.close_reason`). Feeds the attention
      /// note for an agent's `loom stage block`: it crosses the daemon socket, and
      /// `web::model::collect_snapshot` drops it from the browser wire.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub close_reason: Option<String>,

  exactly the pattern of `merge_resolver_session`.
- `data/collector.rs::build_stage_summary` (about line 218): `close_reason:
  stage.close_reason.clone()`.
- `data/sanitize.rs::sanitize_stage_summary`: `summary.close_reason.iter_mut().for_each(flatten);`
  and name `close_reason` in the doc list of untrusted strings (an agent writes it; an ESC or a
  bidi override in it would reach the operator's terminal).
- `web/model.rs::without_merge_resolver_facts` (about lines 214-222): also set
  `stage.close_reason = None;` (one line; the wiring check looks for `close_reason = None`), and
  update its doc comment to say it drops every daemon-only attention input.
- Every `StageSummary { .. }` literal gains `close_reason: None`: `daemon/wire_tests.rs`,
  `ui/tui/ledger/tests.rs`, `ui/tui/ledger/rows.rs` (its test), `ui/tui/state_tests.rs`,
  `data/sanitize.rs` (its test), `web/model_tests_stages.rs`, `render/attention_tests.rs`,
  `render/graph_tests.rs`, `render/attention_model_tests.rs`. Assertion lines stay unchanged.

## 2. The attention note

`attention_model.rs::blocked_guidance`: after the auto-retry arm, a Blocked stage with
`failure_info` `None` and `close_reason` `Some(reason)` gets

    Guidance { note: Some(format!("blocked: {reason}")), ..retry_guidance(stage) }

The note says only "blocked", because the same state follows the stage agent's `loom stage
block`, the operator's own `loom stage block` and `loom stage human-review --reject`, so the
command stays `loom stage retry <id>` (or `loom stage retry <id> --force` at the retry limit,
where this note replaces the "retry limit reached" note) and `automatic` stays `false`. A
Blocked stage with `failure_info` keeps today's guidance. Every block writer clears
`failure_info` (S1 §0), so "no `failure_info`" reliably means one of those three.

## 3. Tests

- `render/attention_model_guidance_tests.rs`: `an_agent_block_shows_its_reason_and_retry`,
  `an_agent_block_at_the_retry_limit_keeps_the_forced_retry`, and
  `a_crash_block_does_not_claim_an_agent_block`.
- `data/sanitize.rs` tests: `a_control_sequence_in_the_close_reason_is_flattened` (mirror
  `a_control_sequence_in_the_merge_resolver_session_is_flattened`).
- `web/model_tests_stages.rs`: the browser snapshot has no `close_reason` key (mirror the
  merge-resolver test at about line 240).
- `daemon/wire_tests.rs`: a `StageSummary` with `close_reason` round-trips through the wire
  encoding. The file is about 375 lines after stage `completion-gates` and the limit is 400:
  set `close_reason` on one stage that `status_data()` already builds and add one test of at
  most 15 lines.

## Check

`cargo test --lib commands::status::render::attention_model`, once. S1 and S3 edit the crate at
the same time: a compile error in a file you do not own is not yours (`common.md`).

## Contracts your code must satisfy

`agent-block-reason-reaches-attention-over-the-wire` and `crash-blocked-stage-is-not-an-agent-block`
(scenarios in the plan's YAML).
