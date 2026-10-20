# Luna assignments: simple tests in disjoint files

These are two separately scheduled assignments for GPT-6 Luna, not permission
to edit another worker's files. Workers do not spawn subagents or run git,
verification, formatters or builds. Report files and unresolved questions.

## Early assignment: deterministic baseline fixture

Own only `loom/src/commands/usage/discovery_tests.rs`.
`explicit_root_discovers_old_mtime_file_without_home_fallback` writes an event
at `2026-09-12T20:00:00Z` but uses `range()`, whose start is `Utc::now() - 1 day`.
The test therefore expires. At planning baseline it discovered the file but
asserted output 1 against parsed output 0.

Give this test an explicit fixed `TimeRange` enclosing its fixed event timestamp.
Keep mtime at UNIX_EPOCH, its explicit fixture root, the discovery/path assertion
and output-token assertion. Do not change production date filtering or the shared
`range()` helper used by unrelated tests. Read `time_range.rs::TimeRange::includes`
and this test's body before editing. The purpose remains proving discovery uses
event time despite ancient mtime. Do not solve it by removing assertions or skips.
The orchestrator's proof is the exact named test, then the full integration gate.

## Late assignment: access-policy unit cases

Own only new `loom/src/commands/status/web/access_tests.rs`. Start after the web
worker settles the access API and registers this module. Read the changed
`access.rs`, `auth.rs` and web brief directly. The web worker owns auth tests,
socket integration tests and all module registrations; do not edit those files.

Use a table of constructed local socket addresses and request headers to assert:

- Numeric authority must match accepted local endpoint and actual port in remote
  mode, including wildcard-listener policy and bracketed IPv6 authority.
- localhost is accepted only at an actual loopback endpoint; arbitrary DNS,
  wildcard destination, bad bracket/port, path and userinfo forms are refused.
- IPv4-mapped IPv6 equivalence follows the API's explicit normalization.
- Remote Origin accepts the matching HTTP authority, refuses other schemes,
  ports, hosts, paths, query/fragment and null, and requires presence for POST/WS.
- Default loopback compatibility remains the existing policy.

Use full boolean/equality assertions for both directions. These are pure tests,
with no listener, network, process, clock or real configuration dependency.
The orchestrator runs the full web unit filter; do not invent a separate public
API or duplicate production parsing in the tests.
