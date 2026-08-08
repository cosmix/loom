# Tests That Cannot Fail

> Topic notes for the mistakes knowledge area.

## A Test Named for a Property Is Not Evidence the Property Is Pinned

**What happened:** three tests written during the tmux-backend plan asserted nothing that could fail.

- `tmux_liveness_ignores_running_server_when_pid_is_dead` (`orchestrator/terminal/tmux/tests.rs`) started **no tmux server at all** — its own comment admitted it. It passed identically whether `is_session_alive` consulted `tmux has-session` or not, i.e. it could not observe the single regression its name claims to pin.
- `list_loom_sockets_ignores_the_overview_viewer_socket` asserted only `is_empty()` — indistinguishable from "the function returns nothing, ever".
- `build_overview_argv`'s pane checks asserted argv *counts* only; tiling session 0 into **every** pane passed the whole suite.

**Why:** a negative assertion ("X ignores Y") is satisfied by a total absence of behaviour. When the setup never reaches the branch, absence is exactly what you get — and a confident test name papers over it. This is the reason it recurred three times in one plan: the name was treated as the evidence.

**Prevention — two-part detection rule:**

1. For every test ask: *if I delete the production line this test covers, does it fail?* If not, it is decorative. Actually delete the line and watch it go red.
2. Every negative assertion needs a **positive control asserted at the same moment**. "The viewer socket was skipped" is only meaningful next to "and a real socket was returned."

**Fix:** the honest version lives in `tests/e2e/tmux_backend.rs`: it asserts BOTH that `tmux has-session` exits 0 (a server genuinely is running — the positive control) AND that `is_session_alive` returns false.

## A Shell-Injection PoC Needs `$(...)`, Not a Trailing `;` Command

**What happened:** `attach/tests.rs` `pane_command_neutralises_hostile_session_ids` used the payload ``$HOME'; touch <probe>; `id`; #``. It never executed the injected `touch` **even with `escape_arg` removed entirely** — the lone unmatched single quote makes `sh` reject the whole line as a syntax error before evaluation. So the test proved nothing about escaping.

**Why:** two independent traps.

- Unbalanced quotes make the *unescaped* control case fail for the wrong reason (parse error, not "injection blocked").
- `exec tmux -L X; touch F` never runs the trailing `touch` either way — `exec` replaces or terminates the process before the `;` is reached (verified with `sh -c 'exec tmux -L x; touch f'`).

**Prevention:** the side-effect trigger in an injection PoC must be `$(...)` or backtick **command substitution**, which runs during word expansion *before* `exec` executes, and the payload must have balanced quotes. Verify the test is real by literally deleting the `escape_arg` call and confirming the probe file appears.

## Filesystem Traversal Order Can Silently Satisfy a Sort Assertion

**What happened:** a test meant to pin `live_tmux_sessions`' `sort_by` wrote fixture files in reverse-alphabetical order — but on macOS/APFS `read_dir` returns entries **already sorted by filename** (verified empirically, 5x repeat). Because the fixture filename equalled the session id, alphabetical filename order already coincided with ascending-id order, so the assertion held even with `sort_by` deleted. `Vec::sort_by` being stable means even a deleted tie-break key preserved the pre-sorted order.

**Prevention:** never let the on-disk filename carry the same ordering key as the logical sort key. Name fixture files independently of the id they embed, so traversal order and logical (`created_at`, `id`) order **structurally disagree** — then the assertion fails without the sort regardless of what order `read_dir` happens to return.
