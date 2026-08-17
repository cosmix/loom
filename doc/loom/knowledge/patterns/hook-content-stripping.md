# Hook Content Stripping

> Stripping heredoc bodies and -m text before matching, the full hook inventory, and the limits of that stripping.

## Hook Content-Stripping Pattern

Hooks that validate bash commands must strip embedded text content before
pattern matching. The strip_embedded_content() function (in hooks/\_common.sh
for shell, validators/bash.rs for Rust) removes:

1. Heredoc bodies (awk state machine tracking <<MARKER to MARKER)
2. -m / --message quoted content (sed replacements)

Each hook sources \_common.sh via: source "$(dirname "$0")/\_common.sh"

Full hook inventory (18 top-level scripts in `hooks/`; 29 including `hooks/tests/`):

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

The `PreToolUse` array in `fs/permissions/hooks.rs` has **13 entries** — several hooks are
registered against more than one matcher (worktree-isolation on Bash/Edit/Write,
worktree-file-guard on Read/Glob/Grep, plans-path-guard on Edit/Write). Its exact length and
per-index order are asserted by `fs/permissions/tests/hooks_tests.rs::test_hooks_config_structure`,
so adding a hook means updating that test too.

## Hook Content-Stripping Pattern (Updated 2026-03-31)

All PreToolUse hooks that match command patterns MUST use `strip_embedded_content()` before pattern matching to prevent false positives from keywords appearing inside commit messages or heredoc bodies.

**Architecture:**

- `_common.sh` provides `strip_embedded_content()` (shared across all shell hooks)
- `loom/src/hooks/validators/bash.rs` provides Rust equivalent `strip_embedded_content()`
- Phase 1: awk state machine strips heredoc bodies (`<<MARKER` to `^MARKER$`)
- Phase 2: sed strips `-m`/`--message` quoted content

**Usage pattern:**

1. Source `_common.sh` at top of hook
2. Call `stripped=$(strip_embedded_content "$cmd")`
3. Use `$stripped` for pattern detection (git -C, .worktrees/, ../../, grep, find)
4. Use original `$cmd` for patterns that MUST match message body (e.g., Co-Authored-By)

**Commit-filter dual-check:**

- STRIPPED_COMMAND for detecting `git commit` (prevents "commit" in messages from triggering)
- ORIGINAL COMMAND for Co-Authored-By check (anchor `^` prevents mid-line false positives)

**Security posture:** All stripping failures result in false positives (overly strict), never bypasses (permissive). This is the correct safety direction for development hooks.

**Hooks using this pattern:** worktree-isolation.sh, commit-filter.sh, git-add-guard.sh, prefer-modern-tools.sh

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
