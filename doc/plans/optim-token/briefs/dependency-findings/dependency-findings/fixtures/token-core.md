<!-- loom-memory-journal -->
# Memory Journal: token-core

**Stage**: token-core
**Created**: 2026-10-14 18:24:55 UTC

---

### ✅ Decision [2026-10-14 18:23:25] id=6258b46c4e064190ad85a641eb822c00

contracts: fixtures use the real current unix time for every tracker contract (status contracts need it); quota_left pure contracts use a fixed now. Contracts were type-checked and clippy-clean against a temporary stub of the common.md section 3 API (removed before freeze).

*Session:* session-c0ba91f7-1792001789

---

### 📝 Note [2026-10-14 18:28:32] id=47e546a7e7524a97b2682ff3e8024845

gotcha: loom subagents watch --worker codex:<unit> exited 5 'unknown: companion request cwd mismatch' from both worktree root and loom/ while loom-codex-forwarder agents ran (Agent tool launched them async despite foreground intent); relied on the Agent completion notifications instead

*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 18:34:51] id=6567087b271d4ca78d6f13a53282c140

found: C4 claude_line treats a present-but-non-numeric cache counter as Unreadable while common.md section 7 'Format drift' names only input_tokens/output_tokens; C2 codex_line defaults malformed cache fields to zero. Check at review.

*Evidence:* `loom/src/tokens/claude_line.rs`
*Session:* session-d4c93a26-1792002297

---

### ✅ Decision [2026-10-14 19:02:59] id=96fd2e8e55214057a3918a3d1be7cb52

token flow agent ids: main-/codex- plus 8 chars, but every agent of a stage whose short id is shared gets its full key (main-<uuid>, codex-<thread id>)

*Context:* Codex thread ids are UUIDv7: the first 8 hex chars are the top 32 bits of the ms timestamp, so two forwards started within ~65 s in one stage share them; duplicate ids would collide as UI keys. Contract codex_job_totals only requires starts_with(codex-01a1396b). loom/src/tokens/flow_build.rs ordered()
*Session:* session-d4c93a26-1792002297

---

### ✅ Decision [2026-10-14 19:02:59] id=e17e92fa716f4b318ac81b4e1c01d3ac

tracker checkpoint is read with crate::fs::safe_read::read_bounded instead of a hand-rolled symlink_metadata + take(8 MiB + 1)

*Context:* read_bounded already opens no-follow at every component, O_NONBLOCK (FIFO cannot block), fstat-checks regular file and enforces the cap at stat and at read; same guarantees, no duplicate reader. loom/src/tokens/checkpoint.rs Checkpoint::load
*Session:* session-d4c93a26-1792002297

---

### ✅ Decision [2026-10-14 19:02:59] id=23c2b6edf7904f9bba88d005e6eb4ea8

a checkpoint write failure still writes flow.json, then fails the pass

*Context:* state starts empty when the checkpoint is unreadable, so publishing a full rescan is correct; the thread logs the error once. A directory squatting checkpoint.json keeps the UI live instead of going stale after FLOW_STALE_SECS. loom/src/tokens/tracker.rs scan_until
*Session:* session-d4c93a26-1792002297

---

### ✅ Decision [2026-10-14 19:02:59] id=8c00112dfd2e4e42b83d56dad63b757c

widened commands/usage/codex_discovery.rs collect_jsonl to pub(crate) besides codex_root

*Context:* the tracker's rollout lookup walks the Codex sessions root; reusing collect_jsonl avoids a second recursive walker. loom/src/tokens/discover.rs rollout_files
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:03:05] id=69000c8c65d942c290f172f94fc90bef

gotcha: crate::quota::cache::read_provider (quota/cache.rs:45) uses fs::metadata + File::open without O_NONBLOCK, so a FIFO at .loom/work/quota/<provider>.json blocks read_snapshot forever: the token tracker pass and the 1 s status collector both call it. Pre-existing; not fixed in token-core.

*Evidence:* `loom/src/quota/cache.rs:45`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:03:05] id=5046c4dc41624c2f9a5783f081d3270f

gotcha: models/forward_receipt/load.rs load_receipts opens with O_NOFOLLOW but blocking, so a FIFO at forward-receipts.jsonl blocks the caller; tokens/discover.rs stage_threads checks symlink_metadata().is_file() first.

*Evidence:* `loom/src/models/forward_receipt/load.rs:55`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:03:05] id=10c848b98afc43b795dbc78906821cae

found: token checkpoint growth risk: each Claude transcript keeps up to 4096 message ids (~55 bytes each in the compact [id,[4 numbers]] form, ~225 KB per long transcript); about 35 long main transcripts in one checkpoint pass MAX_CHECKPOINT_BYTES (8 MiB), after which every pass is a full rescan of every file.

*Evidence:* `loom/src/tokens/accumulate.rs:11`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=838efdbc152645ebbeb99ddaaac5a2dd

mutation: second-pass-counts-nothing-new red under re-read from offset 0 with message table cleared

*Evidence:* `src/tokens/tracker.rs:106`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=ea8693daa0034491bd46e2973a141f4c

mutation: repeated-message-id-counts-once red under sum every line / ignore larger output

*Evidence:* `src/tokens/accumulate.rs:210`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=307b5e6db6e648e1830d4005ae1c8ea8

mutation: torn-last-line-counted-once-after-newline red under offset advanced past torn half

*Evidence:* `src/tokens/tail.rs:46`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=2d6ebd1ceff647acba05c9c8900e059d

mutation: rewritten-transcript-is-rescanned red under no restart on inode/shrink / restart keeping offset

*Evidence:* `src/tokens/tracker.rs:103`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=095e7add64da46fcbe120505c4771b6e

mutation: backlog-lands-in-its-own-buckets red under bucket by read time

*Evidence:* `src/tokens/accumulate.rs:247`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=e72ba0458f874fd8ae704dc3399df563

mutation: codex-job-totals-are-deltas-attributed-to-stage red under cumulative sum / cached not subtracted / wrong stage

*Evidence:* `src/tokens/accumulate.rs:225`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=3a87a7308a5341c0bc6df0fe3d57c1c7

mutation: unreadable-usage-line-is-counted red under silent unreadable

*Evidence:* `src/tokens/accumulate.rs:215`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=83efe7ff41864e41ae237f19ae3fea05

mutation: symlinked-tokens-dir-is-refused red under symlink checks removed

*Evidence:* `src/tokens/tracker.rs:120`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=2fc774273ede42c8bfd88f9d3691445b

mutation: agent-labels-are-sanitized red under raw model / raw agent_type

*Evidence:* `src/tokens/accumulate.rs:207`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=d2f5022df14a45418ff0081c35d047d7

mutation: status-data-carries-fresh-flow red under collector never reading flow

*Evidence:* `src/commands/status/data/collector.rs:368`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=ed7249cc050f46ac9127e968853246b8

mutation: status-data-drops-stale-flow red under stale filter removed

*Evidence:* `src/commands/status/data/collector.rs:374`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=b530c6e2964746f98f6ed8792479e18c

mutation: collector-flattens-flow-strings red under inline_safe loop removed

*Evidence:* `src/commands/status/data/collector.rs:379`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=6357568ad59b476dbe7ca49a137378f9

mutation: corrupt-checkpoint-rescans red under parse error propagated (unwrap)

*Evidence:* `src/tokens/checkpoint.rs:44`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=3cbd7e667bce427fa1fab60b8c8c4d63

mutation: checkpoint-resumes-from-offset red under offset ignored

*Evidence:* `src/tokens/tracker.rs:109`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=5070576c75e34d26b665f19b95fa0332

mutation: rate-is-five-minute-average red under 30s divisor / whole-series / 20-bucket window; STAYS GREEN under 11 and 14 bucket windows (fixture line is 14 buckets back)

*Evidence:* `src/tokens/flow_build.rs:168`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=1d7a88fd927c4ebdb65809342c7fd576

mutation: quota-left-estimates-from-slope red under elapsed dropped / old point kept

*Evidence:* `src/tokens/quota_left.rs:41`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=4986ad6d4c1d4adb80945e926b13ce77

mutation: quota-left-flat-usage-lasts-to-reset red under zero slope not caught

*Evidence:* `src/tokens/quota_left.rs:34`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=fbd47fbd12f94d16bc0c2e97e783eb50

mutation: quota-left-survives-reset-jitter red under split at every resets_at difference

*Evidence:* `src/tokens/quota_points.rs:103`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=4c17c78f637f43d6a828c88d3b116010

mutation: plan-rate-sums-stages-and-expires red under first-stage rate / wrong series len / first-stage total

*Evidence:* `src/tokens/flow_build.rs:42`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=f241522b8bfb406fbb6f2a083efa049d

mutation: oversized-flow-sheds-idle-series red under no shedding / sheds active / no cap

*Evidence:* `src/tokens/flow_build.rs:60`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:16:43] id=3d872e6e88494dc3b32d1c248d3d4ca7

mutation: quota-left-ignores-pre-reset-points red under no splitting; green when only drop-split or only jump-split removed (redundant by fixture)

*Evidence:* `src/tokens/quota_points.rs:106`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:17:03] id=0335283d453f47b6bbf7cc81840af66b

found: frozen contract rate-is-five-minute-average stays green under an 11-14 bucket rate window (outside line sits at now-420s); flow_build.rs uses RATE_BUCKETS=10 correctly. A future contract revision should place the outside line in bucket 11.

*Evidence:* `loom/src/tokens/flow_build.rs:168`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:37] id=7f982dd9d88f455f925a00927120bef6

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/claude_line.rs:63 A present but non-numeric cache counter makes the whole line Unreadable and drops its valid input/output tokens, while codex_line.rs treats the same drift as 0. The spec covers only absent fields. Consider treating a non-numeric cache field as 0 for consistency, or keep it and note the choice in common.md.

*Evidence:* `review round 1`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:37] id=2bbc527ace034a3e8801e0ed8f1b20a8

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/tail.rs:45 read_until has no per-line cap: one very long line, or a growing unterminated tail, buffers up to the whole remaining file in memory and is re-read each pass. The read is also not interruptible by the shutdown flag. Consider a line length cap (skip and count as unreadable) and a stop check between lines.

*Evidence:* `review round 1`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:37] id=3dcfdae15d284a898c1bae75eba68b9b

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/checkpoint.rs:50 save() does not check the 8 MiB MAX_CHECKPOINT_BYTES that load() enforces. A checkpoint written over the cap is ignored by the next load, which runs every pass, so every pass would rescan every file. Consider shrinking the message tables or logging when the written size exceeds the cap.

*Evidence:* `review round 1`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:37] id=370d57e40e564878ae2ef76264a5a23d

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/flow_build.rs:126 The full-key fallback on a shared 8-char short id is not in common.md or w1-engine.md. It is reasonable; document it in the spec so the UI stages know ids can be longer.

*Evidence:* `review round 1`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:47] id=66d457fd0f804384a8bb9d5ac100c868

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/claude_line.rs:63 A present but non-numeric cache counter makes the whole line Unreadable and drops its valid input/output tokens, while codex_line.rs treats the same drift as 0. The spec covers only absent fields. Consider treating a non-numeric cache field as 0 for consistency, or keep it and note the choice in common.md.

*Evidence:* `review round 2`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:47] id=f097ae0affde497eb064280cefa22503

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/tail.rs:45 read_until has no per-line cap: one very long line, or a growing unterminated tail, buffers up to the whole remaining file in memory and is re-read each pass. The read is also not interruptible by the shutdown flag. Consider a line length cap (skip and count as unreadable) and a stop check between lines.

*Evidence:* `review round 2`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:47] id=27ec65e3d9344742ac370207c9ffdc09

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/checkpoint.rs:50 save() does not check the 8 MiB MAX_CHECKPOINT_BYTES that load() enforces. A checkpoint written over the cap is ignored by the next load, which runs every pass, so every pass would rescan every file. Consider shrinking the message tables or logging when the written size exceeds the cap.

*Evidence:* `review round 2`
*Session:* session-d4c93a26-1792002297

---

### 💡 Suggestion [2026-10-14 19:18:47] id=c3fd3c22d7764ccb96b4e53548da7423

/home/dkaponis/src/loom/.worktrees/token-core/loom/src/tokens/flow_build.rs:126 The full-key fallback on a shared 8-char short id is not in common.md or w1-engine.md. It is reasonable; document it in the spec so the UI stages know ids can be longer.

*Evidence:* `review round 2`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:18:51] id=91e95117f3494ed88c7beea27e9d87b9

found: review round 1 suggestions left for follow-up: tail.rs:45 read_until has no per-line cap (one huge line buffers the remaining file each pass); checkpoint.rs:50 save() does not enforce the 8 MiB cap load() enforces, so an oversized checkpoint forces a full rescan every pass; claude_line/codex_line disagree on non-numeric cache counters; flow_build.rs:126 full-key id fallback is not in common.md

*Evidence:* `loom/src/tokens/tail.rs:45`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:27:03] id=bc02b1709bd9463f9dd7287ad2ceb3f7

found: tracker thread now reaches scan_pass (goal-backward edge spawn_token_tracker->run_tracker->guarded_pass->scan_pass). scan_pass's pinned signature has no stop flag, so a thread_local TRACKER (RefCell<TrackerLocal{stop: Option<Arc<AtomicBool>>, warning}>) carries the shutdown flag in (set once by run_tracker) and the pass warning out (guarded_pass takes it on Ok). Thread-local, so parallel test trackers never share a flag. A call inside a catch_unwind closure is still a graph edge from the enclosing fn.

*Evidence:* `loom/src/tokens/tracker.rs:27`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:30:53] id=37868a0191254fe9a9cd1d67004dfc55

mistake: a pub fn doc comment in tokens/tracker.rs linked [`TRACKER`], a private thread_local, failing RUSTDOCFLAGS='-D warnings' cargo doc (rustdoc::private-intra-doc-links). Why: the restructure ran only cargo test. Prevention: name private items in pub docs as plain code spans, never intra-doc links.

*Evidence:* `loom/src/tokens/tracker.rs:53`
*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:35:41] id=d80da47802a64dd986bb9d5856255e27

mistake: piped 'loom stage commit' output through rg, which dropped the LOOM_RELAY_V1 line, so the PostToolUse relay hook never relayed the commit ticket (request status: 'not relayed yet'). Prevention: never filter loom stage commit / memory output; tail -3 keeps the relay line.

*Session:* session-d4c93a26-1792002297

---

### 📝 Note [2026-10-14 19:39:35] id=98a969e464ae436f9393cc6d79876376

wiring: loom/src/tokens/*_tests.rs (accumulate, claude_line, codex_line, discover, flow_build, model, quota_left, quota_points, tail, tracker) are wired as unit-test modules through '#[cfg(test)] #[path = "<name>_tests.rs"] mod tests;' in their parent module; the unwired-file check does not follow #[path] attributes. No downstream wiring is needed.

*Evidence:* `loom/src/tokens/tracker.rs`
*Session:* session-d4c93a26-1792002297

---
