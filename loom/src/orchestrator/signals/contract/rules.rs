//! The `## Your Job` rules of the contract signal.

use crate::orchestrator::signals::helpers::CONTEXT_CEILING_HANDOFF;

/// Rules 1 to 6: what to write and what to do before freezing.
const WRITING_RULES: &str =
    "1. Write each contract test above in its file. You may also write files matching \
     the harness globs and make the scaffold edits listed above. Implement nothing: no \
     production code, no fixes, no refactors.\n\
     2. Each test sets up its scenario and must fail on the wrong implementation its \
     Rejects column names.\n\
     3. Every contract test must fail now, before the implementation exists. A compile \
     or collection failure counts as failing.\n\
     4. Do not commit and do not run `loom stage complete`: the stage belongs to the \
     implementation session after you.\n\
     5. Record mistakes, decisions and surprises with `loom memory note` and \
     `loom memory decision`, as any stage does.\n\
     6. Before freezing, run the repository's formatter over every file you wrote (for \
     Rust, `cargo fmt --all`), and keep every test function under 50 lines and every \
     file under 400 (CLAUDE.md Rule 17): the freeze runs the stage's formatter, lint and \
     search checks over what you wrote (run them first), and a frozen file cannot change \
     afterwards.\n";

/// Append the writer's rules.
pub(super) fn append_rules(content: &mut String, stage_id: &str) {
    content.push_str("## Your Job\n\n");
    content.push_str(WRITING_RULES);
    content.push_str(&freeze_rule(stage_id));
    content.push_str(&closing_rules(stage_id));
    content.push_str("- Stay inside this worktree.\n");
    content.push_str(CONTEXT_CEILING_HANDOFF);
}

/// Rule 7, the freeze: the choice flags it takes, how its wait reads, and
/// when a refusal comes back to the writer instead of going to an adjudicator.
fn freeze_rule(stage_id: &str) -> String {
    format!(
        "7. Finish with `loom stage contracts freeze {stage_id} --test <id>=<identifier> ...`: \
         one `--test` per contract that shows `(you choose)` or whose hint you replace, plus \
         any `--runner` or `--cwd` you choose; a later freeze keeps the flags it does not \
         repeat. The freeze waits for the host run as `loom check` does. While `Host job:` \
         reads `requested` or `pending`, change no file and run the same command again. \
         `passed` means loom froze the contracts and ends the session: then stop. `failed` \
         names what to fix: fix what it names and run it again with the same flags; when it \
         names a file you wrote (a formatter, lint or search check), fix that file. The wait \
         ends with a line saying whether the refusal was returned to you (`n of 3`) or filed. \
         A static refusal is returned to you; it is filed as a `freeze-refusal` dispute on \
         the third static refusal in the session, or when you stay quiet 5 minutes after a \
         refusal, and any other refusal is filed at once. If you believe the stage's contract \
         setup (its contracts or harness) or a format criterion makes the freeze impossible, \
         record why with `loom memory note \"freeze-dispute: <evidence>\"`, run the freeze \
         anyway, and stop once it is filed: a filed refusal goes to an adjudicator, who reads \
         your note and decides. The freeze checks that you changed only contract files, \
         harness files and the scaffold edits, then asks the daemon to run every contract on \
         the host and freeze them.\n"
    )
}

/// Rules 8 to 10: follow-up questions, the plan being out of reach, blocking.
fn closing_rules(stage_id: &str) -> String {
    format!(
        "8. If the Adjudicator Feedback section asks questions, answer each with `loom memory \
         note \"freeze-dispute: answer <n>: <evidence>\"`, then run the freeze and stop; \
         that is your follow-up. Do not commit.\n\
         9. You cannot change the plan, and neither can the operator's answer to a question: \
         do not ask the operator to amend contracts, harness or criteria, or to run commands \
         for you. A refused freeze reaches the adjudicator on its own. Ask the operator only \
         what only a person can decide.\n\
         10. If the work needs something only a person can provide (a credential, a host \
         install), run `loom stage block {stage_id} \"<what and why>\"`.\n\n"
    )
}
