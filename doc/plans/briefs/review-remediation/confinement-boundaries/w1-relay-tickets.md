# confinement-boundaries / w1-relay-tickets: relay line cap, control-ticket provenance, quota text

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md`, sections CR-03, CR-33;
`doc/plans/briefs/review-remediation/validation/runtime.md`, section CR-74 sub-claim (c).

## Findings covered

- CR-03: sixteen forged relay lines crowd out a real ticketed line; a ticket a subagent planted in
  the shared scratch directory is relayed as the lead's when its line shows up in a lead call; a
  refused subagent control ticket is left on disk for a later replay; the quota refusal does not
  name the files that can never be relayed.
- CR-33: `ControlSurfaces::pattern_names` keeps a rule whose path climbs out through `..`, and
  `.codex` is not a control component.
- CR-74(c): the inbox capacity check is a soft cap; nothing says so.

## Files

Own (write):
`loom/src/relay/line.rs`; `loom/src/relay/mod.rs`; `loom/src/relay/emit/stale.rs`;
`loom/src/relay/emit/quota.rs`; `loom/src/commands/hook/relay.rs`;
`loom/src/commands/hook/relay/admission.rs` (new); `loom/src/commands/hook/relay/tests_admission.rs` (new);
`loom/src/commands/hook/relay/ticket_check.rs`; `loom/src/commands/hook/relay/tests.rs`;
`loom/src/commands/hook/relay/tests_ticket.rs`; `loom-hooks/loom-relay-guard.sh`;
`loom-hooks/tests/loom-relay-guard-call-marker.sh` (new); `loom-hooks/tests/run-all.sh`;
`loom/src/sandbox/control_surfaces.rs`; `loom/src/sandbox/control_surfaces/tests.rs`;
`loom/src/fs/inbox/write.rs`;
`loom/tests/integration/relay_control_e2e.rs`; `loom/tests/integration/relay_commit_e2e.rs`;
`loom/tests/integration/relay_dispute_replacement_fixture.rs`; `loom/tests/host_verification_e2e.rs`
(the last four only to add the call marker their real-hook runs now need; see change 5).

Read only: `loom/src/commands/hook/relay/sweep.rs`, `loom/src/commands/hook/relay/output.rs`,
`loom/src/relay/kind.rs`, `loom/src/fs/safe_read.rs`, `loom-hooks/_relay_kinds.sh`,
`loom-hooks/loom-relay.sh`, `loom-hooks/tests/loom-relay-guard.sh`.

Frozen (contract tests, never edit): `loom/src/commands/hook/relay/provenance_contract_tests.rs`,
`loom/src/sandbox/control_surfaces/parent_dir_contract_tests.rs`.

## Changes

`commands/hook/relay.rs` is 384 lines. It must end below 360: move `Request::relay_one` and
`Request::admit` (today lines ~277-354) into the new `admission.rs` as a second `impl Request<'_>`
block, with `relay_one` made `pub(super)`. Declare `mod admission;` beside `mod output;`.
`record` stays in `relay.rs`.

### 1. CR-03 line cap (`relay/line.rs`, `commands/hook/relay.rs::relay_payload`)

- Add `pub fn extract_where(text: &str, mut keep: impl FnMut(&RelayLine) -> bool) -> Vec<RelayLine>`
  to `RelayLine`: parse every line, dedupe by id (first occurrence wins, as today), apply `keep`,
  and stop once `MAX_LINES_PER_CALL` KEPT lines are collected. Rewrite `extract` as
  `Self::extract_where(text, |_| true)`; its doc keeps "capped at `MAX_LINES_PER_CALL`".
- In `relay_payload` replace the `extract(..).into_iter().filter(ticket exists)` chain with
  `RelayLine::extract_where(&collected.text, |line| ticket_path(..).symlink_metadata().is_ok())`.
- `relay/mod.rs`: `MAX_LINES_PER_CALL` doc becomes "At most this many ticketed relay lines are
  relayed from one tool call's output; lines without a ticket file do not count."

### 2. CR-03 control-ticket provenance (`loom-relay-guard.sh`, `admission.rs`)

The marker: `<LOOM_SCRATCH_DIR>/.pre-<tool_use_id>`, an empty regular file. A tool_use_id is valid
when it is 1 to 128 characters, each ASCII alphanumeric, `_` or `-`; any other id gets no marker
and the relay treats it as absent.

Guard side, `loom-hooks/loom-relay-guard.sh`:
- Extend the one jq call (`FIELDS=`) with a second single-line field
  `(.tool_use_id // "" | tostring | gsub("[\r\n]"; " "))` after the tool name, and split it into
  `TOOL_USE_ID` the way `TOOL_NAME` is split.
- Add `write_call_marker` and call it after `check_filter_rule`, before the final `exit 0`, so a
  blocked call never gets a marker. It returns at once unless some line of `relay_each_invocation`
  has a kind for which `relay_kind_is_control` holds (this includes `complete` and
  `subagents:control`, which the guard does not otherwise guard) and `TOOL_USE_ID` matches
  `^[A-Za-z0-9_-]{1,128}$` (put the regex in a variable; Bash 3.2). Then, in order:
  `find "$LOOM_SCRATCH_DIR" -maxdepth 1 -type f -name '.pre-*' -mmin +60 -exec rm -f {} + 2>/dev/null || true`
  (stale markers of calls whose PostToolUse never ran), `rm -f "$marker"`, and
  `(umask 077 && : >"$marker") 2>/dev/null || true`. It does not branch on `agent_type`.
- Update the header comment: the guard also writes the call marker, and why.

Relay side, new `commands/hook/relay/admission.rs`:
- `pub(super) fn marker_path(scratch_dir: &Path, tool_use_id: &str) -> Option<PathBuf>`: `None`
  for an invalid id.
- `pub(super) fn discard_call_marker(scratch_dir: &Path, tool_use_id: Option<&str>)`: remove the
  marker when the id is valid; every error is ignored (the guard's age sweep covers leftovers).
- `relay_one(&self, line)`, in this order:
  1. `admit(line.kind)` as today. When it refuses because `self.agent == AgentRole::Subagent`
     and `line.kind.is_control()`, call `ticket_check::read_verified`; when that verifies, call
     `consume(&ticket_check::ticket_file(..))` and reply
     `LOOM relay: refused {label}: {reason}. Its ticket was deleted.` (when `consume` returns a
     non-empty note, append that note instead of "Its ticket was deleted."). When it does not
     verify, reply with the read error as today and keep the file. Any other refusal keeps the
     ticket, as today.
  2. `read_verified` (now returning the ticket's mtime, change 3).
  3. For `line.kind.is_control()` with `self.agent == AgentRole::Main`, call
     `require_in_call_window(&self.env.scratch_dir, self.tool_use_id.as_deref(), modified, self.env.uid, SystemTime::now())`.
     On `Err(reason)` reply `LOOM relay: refused {label}: {reason}` and keep the ticket.
  4. `self.record(ticket, sweep::Source::Line)` as today.
- `fn require_in_call_window(scratch_dir: &Path, tool_use_id: Option<&str>, ticket_modified: SystemTime, uid: u32, now: SystemTime) -> Result<(), String>`.
  Refusal texts, one per cause:
  - no or invalid id: `this call carries no usable tool_use_id, so its control request cannot be tied to it. Rerun the loom command alone, as its own Bash call`
  - marker missing (`symlink_metadata` NotFound): `no call marker from loom-relay-guard.sh exists for this call, so the ticket cannot be shown to come from it. Rerun the loom command alone, as its own Bash call`
  - marker not a regular file, or not owned by `uid`, or unreadable: `the call marker for this call is not a plain file this user owns`
  - `ticket_modified < marker mtime`: `the ticket was written before this call started, so this call's own command did not write it`
  - `ticket_modified > now`: `the ticket's modification time lies in the future`
- In `relay.rs::relay_payload`, after the `tool_name != "Bash"` early return, move the rest of the
  body into a new private `fn relay_bash_call(value: &Value, env: &RelayEnv, work_dir: &Path, allowed: &[RequestKind]) -> Option<String>`,
  then `let reply = relay_bash_call(..); admission::discard_call_marker(&env.scratch_dir, value.get("tool_use_id").and_then(Value::as_str)); reply`.
- Update the module doc of `relay.rs` (lines 11-15): a control ticket also needs the lead's call
  marker and a modification time inside the call; a refused subagent control ticket is deleted.

Known limit (write it as a comment on `require_in_call_window`): a subagent that rewrites its
ticket continuously while the lead's call runs can still land inside the window. Revisit if
tickets gain a writer identity.

### 3. `ticket_check.rs`

`open_owned` returns `(File, std::fs::Metadata)`; `read_verified` returns
`Result<(Ticket, SystemTime)>`, the `SystemTime` being `metadata.modified()` of the same open
descriptor (error context: `the ticket's modification time could not be read`). `read_unlisted`
keeps its signature and uses `metadata.len()`. Update `tests_ticket.rs:54`
(`let (ticket, _) = read_verified(..)`); no assertion changes.

### 4. CR-03 quota text (`relay/emit/stale.rs`, `relay/emit/quota.rs`)

- `TicketCounts` gains `undecodable: usize` and `undecodable_ids: Vec<String>` (at most
  `MAX_NAMED_IDS`, directory order). A `.req` file is undecodable when it cannot be opened with
  `open_regular_no_follow`, read within `MAX_TICKET_BYTES`, or decoded by `Ticket::decode`, or its
  name is not UTF-8. Its id is the file name without `.req`, `to_string_lossy` for a non-UTF-8 name.
  Undecodable files still count in `live`.
- Replace `stale_control_id` with `fn classify_one(scratch_dir, name: &OsStr, now) -> Class` where
  `enum Class { StaleControl(String), Live, Undecodable }`: decode first, then the age rule
  (control and older than `STALE_AFTER` is stale; everything else that decodes is live).
- In `require_headroom`'s `live` refusal, when `undecodable > 0`, append:
  ` {undecodable} of them do not read as relay tickets and are never relayed: {ids}. Delete `$LOOM_SCRATCH_DIR/<id>.req` for each.`
  The scratch-limit branch is unchanged.
- Update the module doc of `stale.rs` (it says nothing deletes; still true for the CLI).

### 5. Integration fixtures that now need a call marker

Each of these runs the real `loom-relay.sh` on a control kind with a payload that has no
`tool_use_id`, so the relay will refuse it. In each, add `const TOOL_USE_ID: &str = "toolu_e2e";`,
add `"tool_use_id": TOOL_USE_ID` to the hook payload, and write the empty file
`<scratch_dir>/.pre-toolu_e2e` immediately before the loom CLI command that writes the ticket is
spawned (`std::fs::write`, overwriting is fine; each relay call deletes it):
- `tests/integration/relay_control_e2e.rs`: `Fixture::run_hook` payload; marker in `Fixture::block`.
- `tests/integration/relay_commit_e2e.rs`: `run_hook` payload; marker in `stage_and_request_commit`.
- `tests/integration/relay_dispute_replacement_fixture.rs`: `run_hook` payload; marker in
  `Fixture::run_dispute`.
- `tests/host_verification_e2e.rs`: `run_hook` payload; marker in `complete_in_session`.
No assertion in these files changes.

### 6. CR-33 (`sandbox/control_surfaces.rs`)

- `CONTROL_COMPONENTS` becomes `[&str; 6]` with `.codex` added; update its doc to name it.
- In `pattern_names`, right after `let path = expand(..)`, return `true` when
  `path.components().any(|c| c == Component::ParentDir)`, for path rules and Bash words alike.
  Then delete the now dead `|| prefix.components().any(|c| c == Component::ParentDir)` from the
  relative branch (it keeps only `path_rule && prefix.as_os_str().is_empty()`).
- Update the `names` doc: any `..` component marks a rule.

### 7. CR-74(c) (`fs/inbox/write.rs`, `relay/mod.rs`)

Doc only. At the `count_json_entries` check in `write_entry` add: "A soft cap: concurrent relay
hooks of one session each count before any links, so the inbox can overshoot by the number of
concurrent writers. Nothing downstream needs the exact bound." Add the same sentence, shortened,
to `MAX_PENDING_ENTRIES` in `relay/mod.rs`.

## Interfaces shared with other workers

None. No other worker of this stage touches these files.

## Tests

- `commands/hook/relay/tests_admission.rs` (new; declared at the end of `admission.rs` as
  `#[cfg(test)] #[path = "tests_admission.rs"] mod tests;`). Reuse the fixtures in
  `commands/hook/relay/tests.rs`: make `Fixture` (and its fields), `Fixture::new`, `relay`,
  `inbox`, `ticket_exists`, `write_ticket` and `json_entries` `pub(super)`; write a local payload
  builder taking the tool_use_id and agent_type. Tests, each failing today:
  - `forged_lines_do_not_crowd_out_a_ticketed_control_line`: marker `.pre-toolu_a`, then sixteen
    `kind=block` lines with fresh ids and no ticket, then one real Block ticket line; allowed
    `[Block]`. The real entry is in the inbox. (A memory kind would pass today: the sweep recovers
    it.)
  - `a_control_ticket_older_than_the_call_marker_is_refused_and_kept`: Block ticket backdated 30 s
    with `File::set_times`, marker written now. Reply contains "written before this call", no
    inbox entry, ticket still there.
  - `a_control_ticket_without_a_marker_is_refused`.
  - `a_marker_from_another_call_does_not_admit_a_ticket`: marker `.pre-toolu_other`, payload id
    `toolu_a`.
  - `a_refused_subagent_control_ticket_is_deleted`: agent_type `general-purpose`, Block, allowed
    `[Block]`: reply contains "never relayed from a subagent", ticket file gone.
  - `the_relay_removes_its_own_call_marker`: after a relayed call, `.pre-toolu_a` is gone.
  - `a_memory_ticket_needs_no_marker`: a memory line without a marker is still relayed.
- `relay/emit/quota.rs` tests: `names_undecodable_tickets_in_the_refusal`: 32 empty `{n}.req`
  files; the error contains "do not read as relay tickets" and "0".
- `relay/emit/stale.rs` test `a_req_file_with_a_non_utf8_name_counts_as_live`: also assert
  `counts.undecodable == 1`.
- `sandbox/control_surfaces/tests.rs::names_every_control_surface`: add
  `"Edit(~/foo/../.codex/hooks.json)"`, `"Bash(cp x ../.codex/config.toml)"`,
  `"Edit(//home/op/x/../.claude/settings.json)"`, `"Edit(.codex/config.toml)"`,
  `"Edit(~/x/../.claude.json)"`, `"Bash(cp x ~/y/../.claude.json)"`.
- Shell test `loom-hooks/tests/loom-relay-guard-call-marker.sh` (new, `chmod +x`), modelled on
  `loom-hooks/tests/loom-relay-guard.sh` (`env -i`, `mktemp -d "${TMPDIR:-/tmp}/loom-relaymark.XXXXXX"`,
  here-string input). Cases: `loom stage block s1 "why"` with `tool_use_id` `toolu_A` exits 0
  and creates `scratch/.pre-toolu_A`; `loom memory note x` creates no marker; id `../evil` creates
  no file anywhere under the temp root; `loom stage block s1 why > out.txt` exits 2 and creates no
  marker; a `.pre-old` touched to `202001010000` (`touch -t`) is removed by a control call while a
  fresh `.pre-recent` stays. Register it in `loom-hooks/tests/run-all.sh` after the
  `loom-relay-guard` line: `run_test "loom-relay-guard: writes the call marker a relayed control ticket needs" "$SCRIPT_DIR/loom-relay-guard-call-marker.sh"`.
- Contracts to turn green: `forged-lines-crowd-out-a-ticketed-line`,
  `control-ticket-older-than-the-call-marker`, `refused-subagent-control-ticket-is-kept`,
  `dotdot-or-codex-rule-is-propagated`.

## Proof

From `loom/`: `cargo test --lib relay::`, `cargo test --lib sandbox::control_surfaces::`,
`cargo test --test integration relay_`, `cargo test --test host_verification_e2e`; from the
repository root: `bash loom-hooks/tests/loom-relay-guard.sh` and
`bash loom-hooks/tests/loom-relay-guard-call-marker.sh`.

## Traps

- Checking the marker against the ticket's `created_at` field: the ticket body is written by the
  same untrusted session. Use the open descriptor's mtime.
- Writing the marker before the guard's checks: a blocked call would leave one behind.
- Deleting a subagent-refused ticket without verifying it against the line lets any printed line
  delete an unrelated ticket.
- Writing the regression for the line cap with memory tickets: the sweep relays them anyway and
  the test passes on today's code.
- `relay.rs` near 400 lines: move the methods out before adding code.
- `stale.rs` decode-first changes which files are read: every `.req` is now read (bounded); keep
  the no-follow open.
