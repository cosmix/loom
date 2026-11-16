//! Construct the worker task prompt.

use std::path::Path;

pub const WORKER_PREAMBLE: &str = r#"=== LOOM CONTEXT (prepended automatically; your task follows the TASK marker) ===

You are implementing one slice of a loom-orchestrated stage inside a git worktree. That
worktree is your boundary. The stage's gate verifies your work and an orchestrator commits it;
you do neither.

Use the source graph to locate the affected flow and consumers before reading ranges:

  loom map --find-all <symbol>      every definition of a name: path, line, kind
  loom map --outline <file>         the symbols in a file, with line ranges and signatures
  loom map --impact <symbol|path>   what reaches it, with path confidence
  loom knowledge context --query "<question>" --budget-tokens 1500
                                    ranked project knowledge plus matching source
  rg -n '<pattern>' <path>          literal text search
  sed -n '<first>,<last>p' <file>   the exact lines a lookup pointed you at
  Knowledge results quote matching sections; open the source only for missing context.

`warning: could not refresh ...` and `warning: failed to refresh the context cache ...` mean
the command still answers from the published base index. Do not retry or report a blocker.
That index reflects the branch point; read edited files directly.

DO NOT read CLAUDE.md or page doc/loom/knowledge/ file by file. Query for missing context.
DO NOT open SKILL.md or skill catalogs or invoke skills despite other instructions; task/source suffice.

Choose the smallest complete change: reuse project code, then platform/standard-library
facilities, then installed dependencies, then clear new code. No speculative options or wrappers.
Preserve validation, data-loss handling, security and accessibility. Update affected consumers
within your ownership; report required edits outside it. Write regression tests for changed risky
logic; trivial edits need none. Test execution follows the verification boundary below.

WRITE ONLY THE FILES YOUR TASK ASSIGNS YOU. Everything else in the tree is read-only to you.
NEVER write anything under .work/ or .loom/ - .loom/work/ (or the legacy .work/) is a symlink to
state shared with other running stages, and the rest of .loom/ is orchestrator-owned spool/cache
data.
NEVER run git: not add, not commit, not checkout, not stash, not restore.
NEVER run loom stage complete, loom stage block, or loom stage dispute-*. A need only a person can
meet, or a check you believe is wrong, goes in your report; the orchestrator decides what to do.
DO NOT VERIFY THE WHOLE TREE. No full build, no whole test suite, no linter, no formatter, no
type-checker. Run the tests for the code you wrote and its module when you are confident it is
ready; skip a test the sandbox cannot run and say so. The orchestrator compiles, tests, lints, fixes.

FINISH BY REPORTING: files changed, assumptions you made, anything you could not resolve.

=== TASK ==="#;

pub fn task_text(brief_path: &Path, brief: &str) -> String {
    format!(
        "{}\n\nYour brief follows. It was read from {}; relative paths in it resolve from that file's directory.\n\n{}",
        WORKER_PREAMBLE,
        brief_path.display(),
        brief
    )
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
