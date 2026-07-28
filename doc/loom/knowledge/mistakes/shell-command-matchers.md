# Shell Command Matchers

> Separators that never become tokens, forgeable glob lookups, env leakage in hook tests, and three Bash traps.

## Token-Based Shell Matchers: Separators That Never Become Tokens (SYSTEMIC, 2026-07-28)

**What happened:** `hooks/subagent-verify-guard.sh` classifies a Bash command by splitting it
into tokens and deciding whether a "runner" token sits in command position. Three separate
bypass classes shipped and were fixed across two stages, each the same root cause.

**Why:** IFS word-splitting only breaks on whitespace, so anything *glued* to a neighbour never
becomes its own token.

| Bypass class | Example that slipped through | Why |
| --- | --- | --- |
| Redirection read as an argument | `cargo test 2>&1 \| tail -50` | `2>&1` hit the positional arm and looked like a test *filter*, making a project-wide run look scoped |
| Newlines eaten by splitting | `cd loom` + newline + `cargo test` | the newline never became a token, so command position never reset and every line after the first was unreachable |
| Glued metacharacters (13 confirmed) | `echo hi; cargo test`, `echo hi&&cargo test`, `true\|\|cargo build`, `{ cargo test; }`, every `if`/`while`/`for` body | a separator with no surrounding space is absorbed into the adjacent token (`test;` is not `test`) |

**Prevention:** the fix for one of these is not the fix for the others, and a half-applied
normalizer looks correct. For any hook that classifies shell commands by tokenising:

1. Normalise newlines to ` ; ` *before* splitting.
2. Pad `;`, `|`, `(`, `)` and the **pair** `&&` — never a lone `&`, which would split `2>&1`
   into `2>` and `1` and reopen the redirection bypass.
3. Skip redirection tokens explicitly; they are not positional arguments.
4. Track quote state across tokens so a quoted mention (`rg "cargo test" doc/`) stays inert.
5. Probe every separator **both glued and spaced, in both directions**, and feed the matcher
   the repo's own documented command forms — the piped `2>&1 | tail -50` shape that global
   CLAUDE.md Rule 14 mandates is the single most likely real-world bypass.

**Fix:** normalisation lives in one place in `hooks/subagent-verify-guard.sh`; regression cases
are table-driven in `loom/tests/integration/hooks_subagent_verify_guard_cases.rs`.

## Glob + `head -1` Is Forgeable in a Security Gate (2026-07-28)

**What happened:** the integration-verify carve-out resolved its stage file with
`ls WORK_DIR/stages/*-STAGE_ID.md | head -1`. Stage files carry a numeric prefix, so a planted
`00-<stage-id>.md` declaring `stage_type: integration-verify` beat the real `02-<stage-id>.md`
lexicographically and granted a full-suite carve-out to an ordinary stage. Reproduced: exit 0
where it must be 2.

**Why:** `head -1` silently *picks a winner* among duplicates instead of reporting ambiguity.

**Prevention:** the obvious fix — prefer the file whose `id:` field agrees — does **not** work,
because whoever plants the decoy also writes its `id:`. The durable rule for any hook that
grants a privilege by reading a `.work` file: **more than one glob match means ambiguous; fail
safe and do not grant the relaxation.** Consult the file only when exactly one match exists.

**Fix:** ambiguity check in `hooks/subagent-verify-guard.sh`; refusal directions pinned in
`loom/tests/integration/hooks_subagent_verify_guard_carveout.rs`.

## A Fail-Safe Fix Needs a Test That Asserts the REFUSAL (2026-07-28)

**What happened:** the carve-out above shipped with a test for the *granted* direction only.
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
detector in `hooks/_common.sh` then matches even for non-claude-named scripts, and the hook
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
holds. **Detection:** exercise a hook regex with *every literal variant*, not just one.

## `strip_embedded_content` Cannot Strip a Multi-Line `-m` Body (PRE-EXISTING)

**What happened:** phase 2 of `strip_embedded_content` in `hooks/_common.sh` is a line-oriented
pattern for `-m` followed by a quoted run, so a quoted body spanning newlines never matches and
survives into the "stripped" string. `git-add-guard.sh` Pattern 1 then matches the staging verb
followed by an unanchored `.*` spanning the whole line — wrongly blocking a legitimate
"stage one file && commit with a multi-line message that merely *mentions* those flags". It is
not limited to git: it blocked a plain `loom memory note` whose *text* quoted the all-files flags.

**Workaround:** split staging and committing into separate Bash calls, and keep the all-files
flag spellings out of message and note text.
**Real fix (open):** anchor Pattern 1 to the staging command's own argument list instead of the
whole line, and make `strip_embedded_content` quote-state aware across newlines.
