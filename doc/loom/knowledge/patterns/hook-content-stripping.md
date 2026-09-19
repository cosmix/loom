# Hook Command Matching

> How a hook decides what a Bash command invokes
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

Full hook inventory (24 top-level scripts in `loom-hooks/`; 64 including `loom-hooks/tests/`):

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

The finalize bridge hook (`loom-hooks/loom-control-*.sh`) guards the most destructive
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

### Path 2 - the heredoc body is not stripped, so the raw command text decides

`is_completion_command` (`loom-hooks/loom-control-complete.sh:143-216`) strips heredoc bodies
through `strip_embedded_content` first, and a stripped body is ignored only when it is provably
inert. The raw decision still stands (the old raw-substring glob over the lowercased command,
matching `loom` followed anywhere by the verb) when any of these hold:

- a body has no terminator, or the quote-blind `-m`/`--message` rewrite fired;
- an opener is not a single fully quoted `<<'WORD'` / `<<"WORD"` at an unquoted position on a line
  that ends unquoted (an unquoted delimiter, `<<-`, a here-string or a second opener on the line);
- the stripped text holds a comment `#`, a backtick, `$'`, `${`, `((`, `$[` or a line splice;
- `$(`, `<(` or `>(` exists and a stripped line held a `)` (bash 5.2 ends a body inside `$( )` at a
  line reading `EOF)` and then runs the following lines; zsh rejects the same text);
- any command in the stripped text is off the inert-reader allowlist (`cat tee wc head tail cd
mkdir touch echo printf true :`, `loom knowledge`, `loom memory`, `git commit`).

A body fed to a shell, `eval`, `source`, `xargs` or an interpreter (python, perl, node, ruby, bun
and peers) also takes the raw substring test, so a quoted completion command hidden in a python
heredoc is still caught. Tokenizer failure (a bare apostrophe in English prose reads as an
unterminated single quote) falls back to the same raw glob, which matches on mere co-occurrence of
`loom` and the verb; every `loom knowledge update` call already supplies `loom` from its own argv.

Net effect in a knowledge stage: `loom knowledge update|replace-section - <<'EOF'` with a quoted
delimiter and prose free of the constructs above passes; an unquoted `<<EOF` or a body containing a
backtick or `${` does not, and the block message names no completion command you typed.

### Detection and what to do

**Detection:** a hard block naming a finalize command you never typed, on a command that is
obviously not a finalize attempt. Identify which path fired: does the text contain the three-token
shape (path 1), or is the heredoc outside the trusted-inert conditions above (path 2)?

**What to do:** prefer a quoted delimiter and an inert reader, keep the body free of backticks,
`${`, `$'` and comment-hash lines, and use a placeholder for the verb. When a long body still trips
it, extract the section to a file inside the worktree and feed it with `- < file`: the file body is
never part of the Bash command text. Several smaller `loom knowledge update` calls also cost less to
diagnose than one large one.

**What NOT to do.** Do not transform or re-encode the command text so the guard sees something
different from what runs - that is hook evasion, it will be refused by the permission classifier,
and it defeats a control that exists to prevent lost work. Do not write the knowledge file with a
file tool from a path outside the worktree either: `worktree-file-guard.sh` blocks file tools
outside the worktree. The one exception is the session's own harness scratchpad
(`/tmp/claude-<uid>/<project>/<session>/scratchpad/`), allowed by `allow_scratchpad`
(`loom-hooks/worktree-file-guard.sh:174-196`) when the path is canonical and owned by the uid.

**Do not "fix" the raw fallback by loosening it.** A non-match on that branch exits 0 and ALLOWS,
so the fallback is fail-safe by construction, and narrowing it opens a bypass rather than merely
reducing noise: quoting the verb inside an otherwise valid invocation still finalizes the stage
while evading a naive quote-stripped matcher. The heredoc strip above is the narrowing that was
threat-analysed, which is why it trusts a body only under the full list of conditions.
