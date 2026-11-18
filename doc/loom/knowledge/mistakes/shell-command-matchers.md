# Shell Command Matchers

>

## Token-Based Shell Matchers: Separators That Never Become Tokens (SYSTEMIC, 2026-07-28)

**What happened:** `loom-hooks/subagent-verify-guard.sh` classifies a Bash command by splitting it
into tokens and deciding whether a "runner" token sits in command position. Three separate
bypass classes shipped and were fixed across two stages, each the same root cause.

**Why:** IFS word-splitting only breaks on whitespace, so anything _glued_ to a neighbour never
becomes its own token.

| Bypass class                        | Example that slipped through                                                                                          | Why                                                                                                              |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Redirection read as an argument     | `cargo test 2>&1 \| tail -50`                                                                                         | `2>&1` hit the positional arm and looked like a test _filter_, making a project-wide run look scoped             |
| Newlines eaten by splitting         | `cd loom` + newline + `cargo test`                                                                                    | the newline never became a token, so command position never reset and every line after the first was unreachable |
| Glued metacharacters (13 confirmed) | `echo hi; cargo test`, `echo hi&&cargo test`, `true\|\|cargo build`, `{ cargo test; }`, every `if`/`while`/`for` body | a separator with no surrounding space is absorbed into the adjacent token (`test;` is not `test`)                |

**Prevention:** the fix for one of these is not the fix for the others, and a half-applied
normalizer looks correct. For any hook that classifies shell commands by tokenising:

1. Normalise newlines to `;` _before_ splitting.
2. Pad `;`, `|`, `(`, `)` and the **pair** `&&` — never a lone `&`, which would split `2>&1`
   into `2>` and `1` and reopen the redirection bypass.
3. Skip redirection tokens explicitly; they are not positional arguments.
4. Track quote state across tokens so a quoted mention (`rg "cargo test" doc/`) stays inert.
5. Probe every separator **both glued and spaced, in both directions**, and feed the matcher
   the repo's own documented command forms — the piped `2>&1 | tail -50` shape that global
   CLAUDE.md Rule 14 mandates is the single most likely real-world bypass.

**Fix:** normalisation lives in one place in `loom-hooks/subagent-verify-guard.sh`; regression cases
are table-driven in `loom/tests/integration/hooks_subagent_verify_guard_cases.rs`.

## Glob + `head -1` Is Forgeable in a Security Gate (2026-07-28)

**What happened:** the integration-verify carve-out resolved its stage file with
`ls WORK_DIR/stages/*-<stage-id>.md | head -1`. Stage files carry a numeric prefix, so a planted
`00-<stage-id>.md` declaring `stage_type: integration-verify` beat the real `02-<stage-id>.md`
lexicographically and granted a full-suite carve-out to an ordinary stage. Reproduced: exit 0
where it must be 2.

**Why:** `head -1` silently _picks a winner_ among duplicates instead of reporting ambiguity.

**Prevention:** the obvious fix — prefer the file whose `id:` field agrees — does **not** work,
because whoever plants the decoy also writes its `id:`. The durable rule for any hook that
grants a privilege by reading a `.loom/work` file: **more than one glob match means ambiguous; fail
safe and do not grant the relaxation.** Consult the file only when exactly one match exists.

**Fix:** ambiguity check in `loom-hooks/subagent-verify-guard.sh`; refusal directions pinned in
`loom/tests/integration/hooks_subagent_verify_guard_carveout.rs`.

## A Fail-Safe Fix Needs a Test That Asserts the REFUSAL (2026-07-28)

**What happened:** the carve-out above shipped with a test for the _granted_ direction only.
Reverting the ambiguity check from `-eq 1` back to `-ge 1` left the entire suite green.

**Prevention:** when a fix is "we made this fail safe", a happy-path test proves nothing. The
regression test must assert that the **unsafe input is refused** — decoy, wrong stage type, and
missing file all belong in the table.

## Hook Tests Must Scrub the Env the Hook Gates On (2026-07-28)

**What happened:** a helper spawned simulated process trees with `Command::new("bash")` without
clearing the inherited environment, so `LOOM_STAGE_ID` / `LOOM_WORK_DIR` leaked from the outer
test process into every simulated tree. Inside an integration-verify stage session this poisons
the carve-out check: **21 of 32 "must be blocked" cases silently flip to false passes.**

**Prevention:** any test that spawns child processes to simulate a hook's runtime context must
`.env_remove()` every variable the hook reads for gating or carve-out decisions, and apply
intentional `extra_env` **after** the removal so opt-in cases still work.

## `$TMPDIR` Contains "claude" — It Breaks Process-Tree Simulation (2026-07-28)

**What happened:** in this harness `$TMPDIR` is `/tmp/claude-1000`, so `TempDir::new()` puts
every spawned test process's cmdline under a path containing `claude`. The name-based subagent
detector in `loom-hooks/_common.sh` then matches even for non-claude-named scripts, and the hook
falsely fires against itself.

**Fix:** build the temp tree under a claude-free path — `tempfile::Builder::new().tempdir_in()`
rooted at `CARGO_MANIFEST_DIR/target/` — for any test that exercises `loom_is_subagent`.

## Bash: `&` in a `${var//pat/rep}` Replacement Expands to the Match (2026-07-28)

**What happened:** `${S//&&/ && }` on `echo hi&&cargo test` yields `echo hi &&&& cargo test`,
not the intended padding — `&` in the replacement means "the matched text", exactly like `sed`.
The hook then matched nothing, so the fix silently did nothing while reading as correct.

**Fix:** escape it — `${S//&&/ \&\& }`. **Detection:** after any `${//}` whose replacement
contains `&`, `printf` the result once before trusting it; the symptom is a substitution that
appears to run while downstream matching behaves as if unchanged.

## Testing `errexit` Inside `||` Disables the Very Thing Under Test (2026-07-28)

**What happened:** `( set -e; ((count++)); echo ok ) || echo aborted` was used to prove the
`((count++))` errexit trap survived. It cannot fail: a subshell placed in a `||` list has
`errexit` suppressed inside it.

**Prevention:** to test `errexit` behaviour, run the snippet as its own `bash -c` and judge it
by exit status. Never wrap it in `||`, `&&`, or `if` — those are the exact contexts that
disable `set -e`.

## `\b` After an Escaped Period Is Not a Word Boundary in ERE (2026-07-28)

**What happened:** `commit-filter.sh`'s subagent staging regex ends with `\b` after an escaped
period, which in ERE is not a word boundary at end-of-string — so the dot form of staging the
current directory escapes that check while the flag form matches.

**Status:** pre-existing; `git-add-guard.sh` blocks the pattern independently, so protection
holds. **Detection:** exercise a hook regex with _every literal variant_, not just one.

## `strip_embedded_content` Cannot Strip a Multi-Line `-m` Body (PRE-EXISTING)

**What happened:** phase 2 of `strip_embedded_content` in `loom-hooks/_common.sh` is a line-oriented
pattern for `-m` followed by a quoted run, so a quoted body spanning newlines never matches and
survives into the "stripped" string. `git-add-guard.sh` Pattern 1 then matches the staging verb
followed by an unanchored `.*` spanning the whole line — wrongly blocking a legitimate
"stage one file && commit with a multi-line message that merely _mentions_ those flags". It is
not limited to git: it blocked a plain `loom memory note` whose _text_ quoted the all-files flags.

**Workaround:** split staging and committing into separate Bash calls, and keep the all-files
flag spellings out of message and note text.
**Real fix (LANDED 2026-08-11):** see the next section — the raw-string scan was replaced by a
token scan, so the workaround is no longer needed for `git-add-guard.sh`.

## Regexing a Raw Command String Matches Prose Inside Quoted Arguments (2026-08-11)

**What happened:** `git-add-guard.sh` stripped heredoc and `-m` bodies and then ran its three
patterns over the **raw command string**. Quoted argument bodies were never stripped, so a
command that merely _mentioned_ the forbidden forms inside a quoted argument was blocked with no
git invocation anywhere in it. Confirmed: `echo 'Never run git add -A or git add . because it
stages .work'` was blocked. This is the section above one level out — same class, wider blast
radius: `echo`, Write-via-shell, subagent prompt forwarding, and any doc or prompt text about
staging hygiene all tripped it. It blocked real work in this repo, including a probe command
written to _test_ the guard.

**Why:** a regex over a command string cannot tell an argument's **value** from an argument's
**mention**. Stripping more kinds of body is a treadmill; the string is the wrong unit.

**The tempting wrong fix:** blank the interiors of quoted spans before matching. It fixes the
false positive and opens a real hole — bash quoting does not change an argument's value, so
`git add '-A'` and `git add ".work"` start passing. Verified against the pre-fix hook: those two
were **already** allowed, along with `echo $(git add -A)` (Pattern 1's boundary class omitted
`)`). Three silent false negatives sat behind the false positive nobody could miss.

**Prevention:** scan **tokens, not text**. Tokenize with quote/escape state, emit a sentinel at
each command boundary, then check tokens that are genuinely arguments of the invocation you care
about. A quoted mention is one token belonging to `echo`; a quoted real flag keeps its value and
is still caught. When a guard has a loud false positive, look for the quiet false negatives of
the same root cause before fixing only the loud one.

**Fix:** `loom_tokenize_command` in `loom-hooks/_common.sh` (permissive: tolerates any shell text,
returns non-zero only on an unterminated quote) plus `scan_git_add_tokens` in
`loom-hooks/git-add-guard.sh`; the old regex block is retained solely as the unterminated-quote
fallback so protection never drops below its previous level. Cases in
`loom-hooks/tests/git-add-guard-quoting.sh`.

**Swept 2026-08-26 (was "deliberately NOT swept").** The deferral above cost real work: the three
deferred hooks kept blocking codex briefs for months, and the deferral note is what identified
them when it finally bit. `commit-filter.sh`, `prefer-modern-tools.sh` and `worktree-isolation.sh`
now scan tokens too, sharing seven `loom_tokens_*` helpers in `_common.sh` rather than each
re-implementing the walk. `strip_embedded_content` is still unchanged and still runs FIRST — it
strips heredoc bodies, which would otherwise tokenize as real command words.
`subagent-verify-guard.sh` remains on raw strings and still carries this bug class.

**The conversion itself is the dangerous part — see the section below.**

## Sourcing a Bash-Targeted Hook From an Interactive zsh Gives Bogus Results (2026-08-26)

**What happened:** ad-hoc probes of `loom_tokenize_command` produced results that contradicted
each other run to run — the same input appeared to splice `sh -c` payloads in one invocation and
not the next. Nothing was flaky. The probes run via `bash <file>` on a saved script were correct;
the ones typed inline were evaluated by the session's **interactive zsh**, where arrays are
1-based, so every index computed by the walker was off by one and the token dumps were
meaningless. Nearly led to a fabricated "non-deterministic tokenizer" bug report.

**Why:** the default shell here is zsh; `loom-hooks/*.sh` are all `#!/usr/bin/env bash` and are only
ever executed by bash in production. Sourcing one into zsh runs bash-targeted array code under
different semantics with no error.

**Prevention:** never verify a hook by sourcing it inline. Put the probe in a file with a bash
shebang and run it with `bash <file>`, and have the probe print `$BASH_VERSION` if there is any
doubt. When two runs of "the same" check disagree, suspect the interpreter before the code.

## Converting a Raw-String Matcher to Token Scanning Silently Narrows It (2026-08-26)

**What happened:** converting `commit-filter.sh`, `worktree-isolation.sh` and
`prefer-modern-tools.sh` from regex-on-raw-string to argv-token scanning removed the documented
false positives AND opened **seven bypasses**, every one of which the old regex had blocked. The
full suite was green — 3090 Rust tests, 39 hook tests, clippy, fmt — and an adversarial review
found six of them; a self-directed pass afterwards found the seventh. Confirmed bypasses:
`bash -c 'git commit'`, `if git commit; then`, `timeout 60 git commit`, `git $'commit'`,
`exec git commit`, a `loom stage complete` split across two segments, and a triple-nested
`bash -c`.

**Why:** a raw regex matches the words **anywhere**; token scanning matches them only at an argv
position the walker actually reaches. Every construct the walker does not model — a shell's `-c`
payload, a keyword or builtin occupying argv[0], an arg-taking wrapper flag, an alternate quoting
spelling — becomes a hole. The tests measured the false positives being removed; nothing measured
the coverage being given up.

**Prevention:** when replacing a matcher, the acceptance criterion is not "the new tests pass" but
**"every input the old matcher blocked is still blocked."** Keep the old pattern runnable and diff
the two over an adversarial corpus. Concretely: `printf '%s' "$cmd" | grep -qiE '<old pattern>'`
next to the new predicate, and a case list of command-word obfuscations (`command`/`exec`,
keywords, wrappers, `VAR=`, absolute and `./` paths, `$'...'`, separators, `$( )`, pipes,
here-strings, process substitution). A green suite is not evidence here — it never was.

**A bounded walker must fail toward the block.** The `sh -c` splice recursion stopped at depth 2
and still returned success, so a triple-nested payload was trusted as fully walked. Exhausting the
budget now returns failure, which routes callers to the stricter raw-regex fallback. Any fixed
bound has a next level; what matters is which way it fails.

**Fix:** `loom-hooks/_common.sh` — `sh -c` payloads spliced (real shells only: `grep -c`, `sort -c`,
`wc -c` must not match), transparent keywords and command-prefix builtins, arg-taking wrapper
flags, ANSI-C quoting, a substitution-state stack so `$(` inside double quotes no longer aborts
the parse, and the depth-budget fail-safe. Cases in `loom-hooks/tests/common-token-helpers.sh`.

**Two residuals, both pre-existing (the old regex allowed them too, so not regressions):**
`${GITBIN} commit` (parameter expansion at a command position) and `git $'\x63ommit'` (ANSI-C hex
escape). Verify any future "bypass" against the OLD pattern before calling it a regression.

## Bash: `'\\'` in a `case` Pattern Never Matches a Lone Backslash (2026-08-11)

**What happened:** `parse_shell_words` in `loom-hooks/codex-forward-guard.sh` used `'\\')` as the
`case` arm meant to catch a backslash, in three places. In a `case` pattern a quoted string has
its metacharacters disabled, so `'\\'` is the literal **two-character** string `\\` and can never
match the single backslash the parser iterates over. Both the `escape` and `double_escape` states
were therefore unreachable dead code.

**Why it mattered:** a backslash fell through to `*)`, was appended as a literal, and left the
parser in `plain`. The standard `'\''` idiom for embedding an apostrophe then knocked the state
machine out of phase, so the rest of the argument was scanned in `plain` where spaces split words
and `( ) $ * { }` hard-reject. Net effect: **any prompt containing an ASCII apostrophe was
unforwardable**, which silently disabled the whole codex implementation lane. The symptom looked
like a backtick or quoting problem in the prompt, not a dead `case` arm.

**Prevention:** to match a lone backslash in a `case` pattern use `'\'` or unquoted `\\` — both
verified. More generally: an unreachable `case` arm in a state machine is invisible, because the
fall-through arm produces plausible output. When adding a state, assert the state is actually
entered (parse a known input and check the **parsed words**, not just the exit code) — a
pass/fail exit code cannot distinguish "handled correctly" from "never reached".

**Fix:** three patterns in `loom-hooks/codex-forward-guard.sh`; round-trip cases (apostrophe idiom,
`\$`, `\"`, `\\`, trailing lone backslash) in `loom-hooks/tests/codex-forward-guard-quoting.sh`, which
asserts the parsed word content, not merely the hook's exit code.

## Lexical Hooks Blocked Legitimate Work and Were Routed Around

**What happened:** between 2026-10-31 and 2026-11-07 loom's PreToolUse guards blocked 435 deduplicated tool calls across 1,037 sessions, 197 of them in subagents, and almost all were legitimate. Counts by guard and rule:

| Blocks | Guard and rule | Legitimate | What the agent did next |
| --- | --- | --- | --- |
| 123 | `worktree-isolation.sh` path traversal | ~all | same command with absolute paths |
| 96 | `loom-control-complete.sh` exact pinned command (`loom stage complete X 2>&1 \| tail -30`) | yes | the pinned command |
| 33 | `loom-control-complete.sh` bare `loom stage complete X`, the form the rules and signals print | yes | the pinned command |
| 70 | `worktree-file-guard.sh` Read outside the worktree (harness `tool-results/*.txt`, `$TMPDIR/*.log`) | yes | `cat`/`sed` of the same file in Bash |
| 42 | `subagent-verify-guard.sh` (`cargo check`, `cargo build`, `bun run build`, `tsc --noEmit`) | yes | `rustfmt` per file, or code left uncompiled |
| 37 | `worktree-isolation.sh` git override (`git -C .`, `git -C "$SCRATCH/clone"`) | 36 of 37 | dropped `-C`; one `eval` loop became a Python `subprocess` loop |
| 19 | `_read_discipline.sh` large read | yes | identical retry |
| 6 | `commit-filter.sh` subagent git in a scratch fixture repo | yes | rearranged commands |
| 4 | `subagent-verify-guard.sh` gate-once, `cargo test --lib --no-run` | yes | filtered test |
| 3 | `commit-filter.sh` isolation bypass (`unset LOOM_MAIN_AGENT_PID`) | no | correct block |

The 42 verify-guard blocks left one stage's wave-2 code uncompiled at the gate, which then found six failures. The capsule was active and was the real write boundary (71 `Read-only file system` denials in the same window).

**Why:** three causes. (1) Lexical matching with no working directory: `worktree-isolation.sh` matched `../` and `git -C` as text, so `cd <wt>/web/src && sed ../../doc/plans/x` blocked though it resolves inside the worktree. (2) Doctrine printed a command the hook rejected: the rules and signals tell the agent to run `loom stage complete <id>`, and `loom-control-complete.sh` refused that spelling and every `| tail` form. (3) A policy withheld compiler feedback: the verify guard blocked every runner that compiles, so subagents wrote code they never compiled. Each block named no sanctioned route, so agents learned to rewrite the command until it passed, which also trained them to treat guards as obstacles.

**Prevention:**

- A guard blocks only what it decides soundly. Where it cannot decide (unknown cwd after a subshell or splice) it warns, or fails closed only for a write-capable target such as `git -C`.
- Every block message names the sanctioned route, and the rules a hook enforces print only commands the hook accepts.
- Where loom owns the exact command, rewrite it (the completion pin) instead of rejecting it.
- Each relaxation ships with its hostile test: the contract tests assert the hostile variants block, not only that the legitimate form passes.
- Resolve paths physically (`cd -P` on existing prefixes), because every worktree's `.loom/work` is a symlink into the main checkout.
- Never key a hook decision on `TMPDIR`: hooks inherit the daemon's environment, the Bash tool exports `TMPDIR=/tmp/claude-<uid>` only inside each command, and a uid-owned test TMPDIR would class every fixture as scratch. Scratch is `LOOM_SCRATCH_DIR` only.
- Record every block (`loom_record_block`, `loom hook blocks`) so the next false-positive wave is counted, not mined from transcripts.

**Fix:** the hook-false-positives plan: physical resolver and cwd tracking in `_containment.sh`, the completion rewrite, harness-output reads, subagent compile commands, the scratch-repo exemption, the unoutlined-read warning and the block ledger. See [Hook Guard Containment](../architecture/hook-system.md).

## Hook-Guard Delivery Slips: Ledger Parser, Template Trim, Gate Coverage, Spawner List

**What happened:** five slips in one plan.

1. The first ledger parser treated a single blank line as an empty file, because it stripped the final newline before testing for empty input.
2. A codex job trimmed `CLAUDE.md.template` to save bytes and reworded the commit-timing sentence, breaking the `tests_commit_timing` pins; the brief said "trim wording" without listing the pinned phrases.
3. `loom check` passed while completion failed: the gate runs the acceptance criteria only, not the frozen contract tests or the plan's `after_stage` truth checks. A byte trim had dropped a phrase an `after_stage` `rg` required, and the contract temp root sat inside the worktree path.
4. `loom check integration-verify` was run with `run_in_background`; it printed PENDING RELAY and `loom request status` showed the ticket never picked up, because the relay hook reads the `LOOM_RELAY_V1` line only from a foreground command's stdout. Re-running `loom check` while a fix worker still wrote the tree submitted a new host job against a half-edited tree.
5. `loom/tests/hook_false_positive_contracts/ledger.rs` spawned the loom binary directly without registering in `SANCTIONED_SPAWNERS`, so the full suite failed `binary_spawn_guard`; the stage gate had run only filtered test targets.

**Why:** (1) empty input was tested after stripping line terminators; (2) the brief omitted the pinned phrases; (3) the gate's coverage is narrower than completion's; (4) a long gate was treated like a long build; (5) filtered targets never reached the guard test.

**Prevention:** (1) distinguish empty byte input before stripping terminators and cover blank lines in tests. (2) Brief codex trims with the pinned phrases or have it `rg` for pins; run `orchestrator::signals::` after any template edit. (3) Before committing, run the frozen contract test with a non-claude `TMPDIR` and the plan's `after_stage` commands by hand. (4) Run `loom check`, `loom stage complete` and `loom memory` in the foreground; re-run the same command to keep waiting, but only once the tree is settled (wait on subagents with `loom subagents watch` while anyone edits). (5) Register a new test file that spawns the loom binary in `SANCTIONED_SPAWNERS` and its doc comment in the same change, and run `cargo test --test integration binary_spawn_guard`.

**Fix:** each slip was corrected in the same plan; the spawner is registered in `loom/tests/integration/binary_spawn_guard.rs`.
