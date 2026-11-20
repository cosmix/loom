# Brief: wire (status data and fixtures)

Tier: `loom-software-engineer` (sonnet). Wave 2, beside `plan-stats` (it compiles once `plan_stats::read_current` exists; the orchestrator compiles both together). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/status/data/mod.rs`, `loom/src/status/data/collector.rs`, `loom/src/status/data/collector_snapshot.rs`, `loom/src/commands/status/web/feed_split.rs`, `loom/src/commands/status/web/feed_split_tests.rs`, `loom/src/commands/status/web/model_tests.rs`, `web/src/api/fixtures/snapshot.json`, `web/src/api/fixtures/statuses.json`, `web/src/api/frames.ts`.

## Change

1. `StatusData.plan_stats: Option<crate::plan_stats::PlanStatsRecord>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`), filled in `collect_status_data` (`collector_snapshot.rs:22`) from `plan_stats::read_current(work_dir)`. Not freshness gated.
2. `StageSummary` gains `phases: Vec<PhaseSpan>` (skip when empty) and `completed_at: Option<DateTime<Utc>>`, copied from the stage in `collector.rs` (where `elapsed_secs` is set, :200).
3. `feed_split.rs`: classify `plan_stats` as a whole-snapshot key (it changes once); `feed_split_tests.rs` pins every key, so add it there.
4. `frames.ts`: `stageDynamicSchema` is `stageSummarySchema.omit(...)`; keep `phases` and `completed_at` out of the dynamic frame if the static/dynamic split says so (follow how `elapsed_secs` is classified).
5. Fixtures: one stage in `snapshot.json` gets two phase spans and `completed_at`; add a `plan_stats` record to a second fixture only if `model_tests.rs` builds one from serde output; keep `fixture_matches_serde_output` and `fixture_deserializes_into_web_snapshot` green.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib status::data::` and `--lib commands::status::web::` pass.

## Traps

- `web-dialog` owns `web/src/api/schema.ts`; it mirrors your field names exactly: `plan_stats`, `phases[].phase|started_at|ended_at`, `completed_at`. Do not rename.
- `schema_parity.rs` reads `schema.ts`; a key on the Rust side without one in `schema.ts` fails until `web-dialog` lands. Report it if so; do not edit `schema.ts`.
