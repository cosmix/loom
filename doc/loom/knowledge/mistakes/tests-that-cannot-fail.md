# Tests That Cannot Fail

> Topic notes for the mistakes knowledge area.

## A Test Named for a Property Is Not Evidence the Property Is Pinned

**What happened:** three tests written during the tmux-backend plan asserted nothing that could fail.

- `tmux_liveness_ignores_running_server_when_pid_is_dead` (`orchestrator/terminal/tmux/tests.rs`) started **no tmux server at all** — its own comment admitted it. It passed identically whether `is_session_alive` consulted `tmux has-session` or not, i.e. it could not observe the single regression its name claims to pin.
- `list_loom_sockets_ignores_the_overview_viewer_socket` asserted only `is_empty()` — indistinguishable from "the function returns nothing, ever".
- `build_overview_argv`'s pane checks asserted argv _counts_ only; tiling session 0 into **every** pane passed the whole suite.

**Why:** a negative assertion ("X ignores Y") is satisfied by a total absence of behaviour. When the setup never reaches the branch, absence is exactly what you get — and a confident test name papers over it. This is the reason it recurred three times in one plan: the name was treated as the evidence.

**Prevention — two-part detection rule:**

1. For every test ask: _if I delete the production line this test covers, does it fail?_ If not, it is decorative. Actually delete the line and watch it go red.
2. Every negative assertion needs a **positive control asserted at the same moment**. "The viewer socket was skipped" is only meaningful next to "and a real socket was returned."

**Fix:** the honest version lives in `tests/e2e/tmux_backend.rs`: it asserts BOTH that `tmux has-session` exits 0 (a server genuinely is running — the positive control) AND that `is_session_alive` returns false.

## A Shell-Injection PoC Needs `$(...)`, Not a Trailing `;` Command

**What happened:** `attach/tests.rs` `pane_command_neutralises_hostile_session_ids` used the payload ``$HOME'; touch <probe>; `id`; #``. It never executed the injected `touch` **even with `escape_arg` removed entirely** — the lone unmatched single quote makes `sh` reject the whole line as a syntax error before evaluation. So the test proved nothing about escaping.

**Why:** two independent traps.

- Unbalanced quotes make the _unescaped_ control case fail for the wrong reason (parse error, not "injection blocked").
- `exec tmux -L X; touch F` never runs the trailing `touch` either way — `exec` replaces or terminates the process before the `;` is reached (verified with `sh -c 'exec tmux -L x; touch f'`).

**Prevention:** the side-effect trigger in an injection PoC must be `$(...)` or backtick **command substitution**, which runs during word expansion _before_ `exec` executes, and the payload must have balanced quotes. Verify the test is real by literally deleting the `escape_arg` call and confirming the probe file appears.

## Filesystem Traversal Order Can Silently Satisfy a Sort Assertion

**What happened:** a test meant to pin `live_tmux_sessions`' `sort_by` wrote fixture files in reverse-alphabetical order — but on macOS/APFS `read_dir` returns entries **already sorted by filename** (verified empirically, 5x repeat). Because the fixture filename equalled the session id, alphabetical filename order already coincided with ascending-id order, so the assertion held even with `sort_by` deleted. `Vec::sort_by` being stable means even a deleted tie-break key preserved the pre-sorted order.

**Prevention:** never let the on-disk filename carry the same ordering key as the logical sort key. Name fixture files independently of the id they embed, so traversal order and logical (`created_at`, `id`) order **structurally disagree** — then the assertion fails without the sort regardless of what order `read_dir` happens to return.

## Five More Instances In One Plan — This Is the Repo's Most Recurrent Defect Class

The context-retrieval plan produced five fresh instances across three stages. At that
frequency the class is not bad luck; treat every new test as guilty until you have
answered the deletion question above. Each shape below is distinct and worth
recognising on sight.

### 1. A feature-flag suite that only exercises the OFF path

`native/launch.rs` `resolve_prompt_cache_split_prefix_file` returned `None` on
**every** path, including flag-ON, so the prompt-cache-split feature was structurally
inert — and both of its pinned acceptance greps still passed.

**The misleading signal is specific and worth memorising:** "ships DISABLED by
default" makes an always-`None` resolver look like *correct behaviour* rather than a
dead path. Disabled-by-default means the flag defaults off, NOT that the enabled
branch can never work.

**Rule:** every flag needs at least one ON-path test asserting the observable effect
(a file written, an argument on the command line) — a suite that only drives the off
path cannot tell "disabled" from "broken".

### 2. A formatter test that hand-builds its own input

`orchestrator/signals/tests_brief.rs` asserted the recovery signal renders a
Knowledge Brief from a hand-made `EmbeddedContext`, while the only production caller,
`generate_recovery_signal`, never set `context_pack`. Every retry therefore shipped a
signal whose prefix said "read the Knowledge Brief first" with no brief in the file —
fully green.

**Rule:** for every `if let Some(x)` gate in a formatter, grep the PRODUCERS of `x`
and confirm each one sets it. If a struct is built by more than one path, at least one
test must drive the real generator end to end over a temp `.work/`. A renderer test
proves the renderer works and says nothing about whether anything populates it.

### 3. A root-only fixture that cannot distinguish a resolver from string equality

`rank.rs` compared a raw markdown link target against a chunk root-relative path.
Markdown targets are relative to the CONTAINING directory, so `mistakes/a.md` linking
to `verification-harness.md` never matched the chunk at
`mistakes/verification-harness.md` — the link-neighbour boost was dead for every
tier-2 file in the real knowledge base. The unit test passed because its fixture
chunks all sat at the tree root, where the flat comparison happens to be correct.

**Rule:** any test over relative paths MUST include at least one file in a
subdirectory linking to a sibling AND one using a `../` parent hop. A root-only
fixture cannot tell a correct resolver from a string comparison.

### 4. A unit test that passes because the fixture is unrealistically clean

Substring matching for exact-symbol boosts made single-character backticked
identifiers universal matches: knowledge prose contains tokens like `n`, `rg`, `pub`,
and `query.text.contains(symbol)` let `n` match every query containing the letter n,
awarding an 80-point boost labelled `confidence: high`. A mistakes entry about `rg`
flags ranked #1 for the query "signal generation".

**Why tests could never catch it:** they used realistic multi-character symbols like
`ContextStore`. Only a query against the REAL knowledge base exposed it.

**Rule:** an "exact match" rung must compare TOKENS, not substrings. And when a
ranking or matching heuristic is only ever tested on tidy fixtures, run it once
against real production data before believing it.

### 5. An equality assertion that pins a broken string

`brief.rs:200` asserts the rendered brief footer equals
`loom knowledge context --stage stage-1 --query "<question>" --budget-tokens <n>` —
and that command does not work, because `loom knowledge context` has no `--stage`
flag. The test pins the defect in place: an equality test on advertised command text
can never notice the command is invalid. See `concerns.md`.

**Rule:** when generated output tells a human or an agent to RUN a command, the test
must execute or parse that command with the real argument parser, never string-match
it. This generalises past CLIs — any test asserting that output equals a literal is
only as correct as the literal.

## The Common Root Cause

Four of the five passed because the test's INPUT was constructed by the test itself,
so the test and the production path never met. The deletion question ("delete the
production line — does it go red?") catches all of them, and the cheapest structural
habit that prevents them is: **assert against data a production code path produced,
even in a unit test.** Where that is impossible, say in the test name or a comment
that it is a structural guard only — a claim the containment stage made explicitly
when no injection seam existed for a fail-closed hook site, which is the honest way
to ship one.
