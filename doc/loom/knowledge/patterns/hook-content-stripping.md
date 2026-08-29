# Hook Command Matching

> How a hook decides what a Bash command actually invokes: strip embedded content, tokenize into
> argv, match command words and argument values — and fall back to the old regexes when the
> command will not parse.

## Match Tokens, Not Text

A hook that validates a Bash command must answer "what does this command INVOKE?", and a regex
over the command string cannot: it cannot tell an argument's _value_ from its _mention_. Text
quoted inside a command — a task brief, a `loom memory note` body, a doc string — is scanned as if
it were shell. That produced months of false blocks (see
`mistakes/shell-command-matchers.md`).

The matching pipeline, in order:

1. **Strip embedded content.** `strip_embedded_content()` removes heredoc bodies (awk state
   machine, `<<MARKER` to `MARKER`) and `-m` / `--message` quoted text. This still runs FIRST and
   is still necessary: a heredoc body is not quoted, so its words would otherwise tokenize as real
   command words. Known limit: it cannot strip a multi-line `-m` body.
2. **Tokenize.** `loom_tokenize_command` walks the stripped string with quote/escape state and
   fills `LOOM_TOKENS` with argv-shaped words plus a `%%SEP%%` sentinel at every command boundary.
   It returns non-zero only when the string ends inside an unterminated quote.
3. **Match tokens.** Ask whether a segment INVOKES a command (`loom_tokens_invoke`) and whether
   that segment carries a given argument (`loom_tokens_cmd_has_arg`, `..._has_arg_pair`,
   `..._cmd_argv`), or whether any word-shaped token matches (`loom_tokens_word_matches`).
   Quoting changes an argument's VALUE, not what matches: `git "commit"` is still caught, while
   the same words inside one quoted argument are one token belonging to `echo`.
4. **Fall back.** When tokenizing fails, run the hook's ORIGINAL regexes verbatim, so protection
   is never weaker than before the conversion. The command is not valid bash anyway.

**Path checks key on whitespace, not quoting.** A real path argument is a whitespace-free word; a
prose payload is not. `loom_token_is_word` is that discriminator — so a quoted real path
(`cat "../../x"`) is still blocked while a brief mentioning `../../src/y` is not.

**Which hooks do this:** `git-add-guard.sh`, `commit-filter.sh`, `worktree-isolation.sh`,
`prefer-modern-tools.sh`, and the finalize bridge. `subagent-verify-guard.sh` is the ONLY hook
still matching raw strings and still carries the bug class — see `concerns.md`.

**New command-matching logic must scan tokens.** Do not add a regex over the raw or stripped
string; the stripped-string regexes survive only as the unterminated-quote fallback.

**Converting a hook is not mechanical.** The 2026-08-26 conversion of three hooks removed the
false positives and opened seven bypasses the old regexes had blocked, none of which a fully green
suite revealed. Read `mistakes/shell-command-matchers.md` § "Converting a Raw-String Matcher to
Token Scanning Silently Narrows It" first.

**Security posture:** every failure mode here — a strip that misses, a parse that aborts, a
recursion budget exhausted — resolves toward the stricter check, never toward permitting. That is
the correct direction for a development guard.

Each hook sources `_common.sh` via `source "$(dirname "$0")/_common.sh"`. The Rust twin of
`strip_embedded_content` lives at `loom/src/hooks/validators/bash.rs` and has NOT been converted.

Full hook inventory (24 top-level scripts in `hooks/`; 64 including `hooks/tests/`):

- PreToolUse: worktree-isolation.sh, commit-filter.sh, subagent-verify-guard.sh,
  git-add-guard.sh, prefer-modern-tools.sh, worktree-file-guard.sh,
  plans-path-guard.sh, ask-user-pre.sh
- PostToolUse: post-tool-use.sh, ask-user-post.sh
- Stop: commit-guard.sh, learning-validator.sh
- SessionStart: session-start.sh
- SessionEnd: session-end.sh
- PreCompact: pre-compact.sh
- UserPromptSubmit: skill-trigger.sh
- Library: \_common.sh (sourced, not registered)
- Git-side: git-pre-commit-hook.sh (appended to `.git/hooks/pre-commit` by `loom init`;
  the only top-level script not in `LOOM_HOOKS`)

The `PreToolUse` array in `fs/permissions/hooks/config.rs` has **35 entries** — most hooks are
registered against more than one matcher (worktree-file-guard on Edit/MultiEdit/Write/
NotebookEdit/Read/Glob/Grep, plans-path-guard on Edit/MultiEdit/Write, codex-forward-guard on
Bash/Edit/Write/Read/Task/Agent, stage-terminal-guard on Write/Edit/Task/Agent). Its exact length
and the per-index order of its first sixteen entries are asserted by
`fs/permissions/tests/hooks_tests.rs::test_hooks_config_structure`, so adding a hook means updating
that test too.

**Commit-filter's dual read is still load-bearing.** It matches TOKENS to decide whether a real
`git commit` is being invoked, but scans the ORIGINAL command for attribution trailers — those
exist precisely inside the message body, so stripping or tokenizing would blind the check. Detect
the invocation on tokens; inspect message content on the raw string.

## Two Ways The Stage-Finalize Prefilter Blocks A Command You Never Typed

The finalize bridge hook (`hooks/loom-control-*.sh`) guards the most destructive
operation in loom, so it fails closed: anything its prefilter matches must be
byte-identical to the pinned invocation or it is rejected. The prefilter was
hardened to tokenize the Bash command instead of globbing the raw string, and that
fix is real - matching now happens on argv VALUES at command positions, so quoting
can neither forge nor evade it (`is_completion_attempt`, lines 47-111).

Both remaining false-positive paths were reproduced from a knowledge-distillation
stage, the stage type most exposed to them because its whole job is feeding long
prose through the loom CLI.

### Path 1 - the tokenizer succeeds and matches your EXAMPLE

The matcher looks for three tokens at a command position: argv[0] whose basename
contains `loom`, argv[1] exactly `stage`, and argv[2] containing the finalize verb.
Prose that spells that shape out verbatim - in a heredoc, a doc comment, or a
knowledge file - tokenizes into exactly those three tokens and matches. The matcher
is behaving correctly; a quoted example is indistinguishable from the real
invocation.

**So the pinned triple cannot be written literally inside any Bash command.**
Writing this very section was blocked twice before the examples were replaced with
placeholders. Render it as `loom stage <VERB> x` and name the verb in prose.

### Path 2 - the tokenizer FAILS and the old glob returns

When `loom_tokenize_command` cannot tokenize, line 139 falls back to a raw
substring glob over the lowercased command, matching `loom` followed anywhere by
the verb - deliberately, so the gate is never weaker than it was before the
tokenize fix. A heredoc body is scanned as ordinary command text, and **a bare
apostrophe in English prose reads as an unterminated single quote**, which alone is
enough to fail tokenization. The fallback then matches on mere co-occurrence of the
two words, and every `loom knowledge update` call already supplies the first one
from its own argv.

Reproduced with a matched pair, both fed to a harmless `wc -c` and differing by one
character: a body reading `loom<APOSTROPHE>s own docs say the word <verb> here` was
BLOCKED, while the identical body with the apostrophe removed was allowed.

Balanced quotes are fine - with quoting intact the tokenizer succeeds and quoted
prose is ignored entirely. This is the narrowed residual of the trap that hit the
verification stage four times in one session.

### Detection and what to do

**Detection:** a hard block naming a finalize command you never typed, on a command
that is obviously not a finalize attempt. Identify which path fired: does the text
contain the three-token shape (path 1), or an apostrophe plus both words (path 2)?

**What to do:** write the prose so neither path fires - use a placeholder for the
verb, and avoid apostrophes in any heredoc fed to a loom command (write "does not"
rather than the contraction). Prefer several smaller `loom knowledge update` calls
over one large one, so a block costs less to diagnose and redo.

**What NOT to do.** Do not transform or re-encode the command text so the guard
sees something different from what runs - that is hook evasion, it will be refused
by the permission classifier, and it defeats a control that exists to prevent lost
work. Do not route around it by writing the knowledge file with the Write tool from
a path outside the worktree either: `worktree-file-guard.sh` blocks file tools
outside the worktree, scratchpad directories included.

**Do not "fix" path 2 by loosening the fallback.** A non-match on that branch exits
0 and ALLOWS, so the fallback is fail-safe by construction, and narrowing it opens
a bypass rather than merely reducing noise. The real fix is to strip heredoc bodies
before matching - the stripping this topic file documents elsewhere - and it needs a
threat analysis first, because quoting the verb inside an otherwise valid invocation
still finalizes the stage while evading a naive quote-stripped matcher.
