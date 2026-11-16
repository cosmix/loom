//! Native-lane text of the Codex Implementers section.
//!
//! On the native lane the stage's main agent launches Codex workers itself with
//! `loom subagents start`; no forwarder exists. The intro, the lane-scope bullet,
//! the verification bullet and the watch exit meanings are shared with the
//! legacy text (private items of the parent module).

use super::{
    push_codex_intro, push_codex_scope_bullet, push_codex_verification_bullet,
    push_watch_exit_meanings,
};
use crate::codex::{
    CODEX_IMPLEMENTER_EFFORT, CODEX_IMPLEMENTER_MODEL_LUNA, CODEX_IMPLEMENTER_MODEL_SOL,
    CODEX_IMPLEMENTER_MODEL_TERRA,
};
use crate::models::stage::Implementers;

/// Push the whole native-lane doctrine after the section heading.
pub(super) fn push_native_doctrine(content: &mut String, implementers: &Implementers) {
    push_codex_intro(content, implementers);
    push_start_bullets(content);
    push_wait_bullet(content);
    push_harvest_and_steer_bullets(content);
    push_sizing_bullets(content);
    push_brief_and_blast_bullets(content);
    push_codex_scope_bullet(content);
    push_codex_verification_bullet(content);
}

/// Push the lane statement, the start command form and the start verdict.
fn push_start_bullets(content: &mut String) {
    content.push_str(concat!(
        "- THIS STAGE'S CODEX LANE IS NATIVE. You start each Codex unit yourself with `loom subagents start`;\n",
        "  no forwarder exists on this lane, so never spawn `loom-codex-forwarder` or the plugin's\n",
        "  `codex:codex-rescue`. Only YOU, the stage's main agent, can run `loom subagents start`,\n",
        "  `loom subagents message` and `loom subagents cancel`: a ticket from a subagent is refused, so\n",
        "  never hand these commands to a coordinator.\n",
    ));
    content.push_str(&format!(
        concat!(
            "- START each unit with `loom subagents start --provider codex --unit-id <unit-id> --brief <absolute brief path> --model <model> --effort {EFFORT}`,\n",
            "  ALONE in its own Bash call: never chained, piped or backgrounded, because a hook pins the command\n",
            "  form. Give every logical Codex unit a stable unique id matching `[A-Za-z0-9][A-Za-z0-9_-]{{0,63}}`\n",
            "  (no dot). The brief is a file inside the stage worktree of at most 262144 bytes. State the model\n",
            "  in the command, one per tier:\n",
            "  `--model {TERRA}` for implementation, integration tests, boilerplate, scaffolding and simple unit tests;\n",
            "  `--model {SOL}` for a substantial well-specified unit (about 4 to 6 file-sized changes) or a terra unit that failed once;\n",
            "  `--model {LUNA}` only when the user or plan asks.\n",
        ),
        EFFORT = CODEX_IMPLEMENTER_EFFORT,
        TERRA = CODEX_IMPLEMENTER_MODEL_TERRA,
        SOL = CODEX_IMPLEMENTER_MODEL_SOL,
        LUNA = CODEX_IMPLEMENTER_MODEL_LUNA,
    ));
    content.push_str(concat!(
        "- The start command only writes a relay ticket and exits 0; its last output line names the request id.\n",
        "  Read the verdict in a SEPARATE Bash call: `loom request status <id> --wait 90`. It prints\n",
        "  `native worker <unit-id> admitted as <invocation id>` (keep that invocation id: message and cancel\n",
        "  need it) or `native worker <unit-id> already admitted as <invocation id>` when the identical start\n",
        "  was repeated. `refused: <reason>` means nothing started: fix the cause and start again. Repeating\n",
        "  a start for a unit that still runs with the same brief, model and effort is safe; a different\n",
        "  request for it is refused. The host admits at most 6 running workers per stage.\n",
    ));
}

/// Push the one-watch-per-wave rule with the shared exit-code meanings.
fn push_wait_bullet(content: &mut String) {
    content.push_str(concat!(
        "- WAIT with ONE background `loom subagents watch --worker codex:<unit-id> --timeout 3600` for the WHOLE\n",
        "  wave: start every unit first, then pass one `--worker codex:<unit-id>` per admitted unit (and one\n",
        "  `--worker claude:<agent-id>` per Claude subagent in the wave) through the Bash tool's\n",
        "  `run_in_background`. Never use a model-driven status loop and never re-arm the watch. The watch\n",
        "  binds those workers once, holds one lease for the parent session,\n",
        "  prints one initial record and one terminal record, then exits. Treat exits distinctly: 0 only\n",
    ));
    push_watch_exit_meanings(content);
    content.push_str(concat!(
        "  For a native worker, exit 6 means no event past the stall budget: cancel it yourself (below); it is\n",
        "  stopped only when its terminal record says so. Then re-split its remainder.\n",
    ));
}

/// Push the harvest rule and the steer/cancel commands.
fn push_harvest_and_steer_bullets(content: &mut String) {
    content.push_str(concat!(
        "- HARVEST each unit once: `loom subagents result --worker codex:<unit-id>` prints its final report\n",
        "  (`--full` for the whole text, `--json` for a record); `loom subagents status --worker codex:<unit-id>`\n",
        "  is a one-shot diagnostic. Accept a unit only after the watch reported it succeeded (exit 0). A worker\n",
        "  whose processes could not be proven stopped (exit 5) is NEVER success: it may still be writing. Do not\n",
        "  restart that unit; inspect the worktree and ask the operator to run `loom subagents release`. After\n",
        "  each unit run `git status --short`: anything touched outside its files is yours to catch.\n",
        "- STEER OR STOP a running worker by its invocation id, each command ALONE in its own Bash call, then\n",
        "  read the verdict with `loom request status <id> --wait 90` in another:\n",
        "  `loom subagents message --worker codex:<unit-id> --invocation <invocation id> --delivery steer|next-turn --message-id <message-id> --text <text>`\n",
        "  (`--file <path>` for a longer text; at most 16384 bytes; `steer` reaches the running turn, `next-turn`\n",
        "  waits for it to end; repeat a message only with the identical id and text; ledger line\n",
        "  `message <message-id> accepted for <invocation id>`), and\n",
        "  `loom subagents cancel --worker codex:<unit-id> --invocation <invocation id> --reason <text>`\n",
        "  (ledger line `cancel requested for <invocation id>`). Both are refused for an invocation that is\n",
        "  not the unit's running one.\n",
    ));
}

/// Push the unit-sizing, cohesion and fan-out rules.
fn push_sizing_bullets(content: &mut String) {
    content.push_str(&format!(
        concat!(
            "- A unit is ONE CONCERN: code, consumers and tests together in dependency order, in one brief. It may\n",
            "  span several files; two units never write one file. Size per file-sized change at high effort:\n",
            "  {SOL} takes about 3 minutes and 50,000 tokens of context; {TERRA} about 1 minute and\n",
            "  20,000 tokens. Each unit starts near 23,000 tokens. Budget twice the time at xhigh. Keep peak context\n",
            "  at most about 250,000 tokens and finish well inside the 1500000 ms deadline: about 4 to 6\n",
            "  file-sized changes on sol, about 12 on terra. At the deadline the host cancels the worker and it ends\n",
            "  timed out: the unit was too large. A timed-out unit is NEVER restarted as is: re-split against its\n",
            "  partial tree, giving each part its own unit id. A retry of the same logical unit keeps the SAME unit id.\n",
        ),
        SOL = CODEX_IMPLEMENTER_MODEL_SOL,
        TERRA = CODEX_IMPLEMENTER_MODEL_TERRA,
    ));
    content.push_str(concat!(
        "- Fill only cohesive units: every turn re-reads the whole context, so cost grows with its length squared.\n",
        "  Never bundle unrelated concerns to reach a size. Waves follow real dependencies only: order work\n",
        "  inside a unit first; a later wave exists only when a unit needs another unit's result on disk. Never\n",
        "  slice units into waves by the cap of 6.\n",
        "- PARALLEL FAN-OUT: run up to 6 workers at once, each owning a DISJOINT file set, in the same\n",
        "  file-ownership table as any Claude subagents in the wave; codex and Claude units may share a wave.\n",
    ));
}

/// Push the brief-writing, navigation-kit and blast-radius rules.
fn push_brief_and_blast_bullets(content: &mut String) {
    content.push_str(concat!(
        "- WRITE THE BRIEF LIKE A SONNET ONE, PLUS ANCHORS: a file naming files owned/read, entry symbols, the\n",
        "  done condition and proof command, and traps; pass its absolute path to `--brief`. Loom copies the\n",
        "  file at admission and the worker reads that copy, so a later edit does not reach it. Declared skills\n",
        "  are for Claude workers: quote applicable rules in the brief; never prepend the Claude preamble.\n",
        "- CODEX ALREADY CARRIES A NAVIGATION KIT: every worker gets `loom map --find-all`, `loom map --outline`,\n",
        "  `loom map --impact` and `loom knowledge context --query` anchors, and reads AGENTS.md, never CLAUDE.md\n",
        "  or doc/loom/knowledge/ - do not re-paste that guidance. Its one blind spot: the index reflects the\n",
        "  branch point, so it cannot see a sibling worker's edits this session - name the file.\n",
        "- BLAST RADIUS: a worker runs with approval `never` in a boundary with a private Codex home (no operator\n",
        "  Codex config, rules, hooks or MCP servers load) and edits anything under the worktree without asking,\n",
        "  except `.loom`, `.git` and `.claude`, which are read-only. No Loom or Claude hook runs for its commands\n",
        "  and it reaches no network but the allowlist proxy. It gets no git and no `.loom/`: NEVER give it a path\n",
        "  under `.loom/work/`, and tell it not to run git at all (the read-only `.git` enforces that; the\n",
        "  instruction is a second layer). While any worker of the stage runs, or its processes could not be proven stopped,\n",
        "  `loom stage reset`, `loom stage retry` (even with `--force`), `loom stage skip`, `loom stage commit`,\n",
        "  `loom check` and `loom stage complete` are refused, naming the worker: cancel it and wait for its\n",
        "  terminal record first.\n",
    ));
}

#[cfg(test)]
mod tests {
    use super::super::format_codex_implementers_section;
    use crate::codex::{
        CodexLane, CODEX_FORWARD_SENTINEL, CODEX_IMPLEMENTER_MODEL_LUNA,
        CODEX_IMPLEMENTER_MODEL_SOL, CODEX_IMPLEMENTER_MODEL_TERRA,
    };
    use crate::models::stage::{Implementer, Implementers};
    use crate::native_worker::ids::{MAX_ACTIVE_PER_STAGE, NATIVE_DEADLINE_SECS};

    fn native_text(implementers: &Implementers) -> String {
        format_codex_implementers_section(implementers, &CodexLane::Native)
    }

    fn collapsed(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn codex_only() -> Implementers {
        Implementers::new(vec![Implementer::Codex])
    }

    #[test]
    fn native_text_names_every_command() {
        let text = collapsed(&native_text(&codex_only()));
        for needle in [
            "loom subagents start --provider codex",
            "loom subagents watch --worker codex:<unit-id>",
            "loom subagents result",
            "loom subagents status",
            "loom subagents message",
            "loom subagents cancel",
            "--timeout 3600",
            "loom request status <id> --wait 90",
            "admitted as",
            "at most 6 running workers",
            "1500000 ms deadline",
            "git status --short",
            ".loom/work/",
            "SAME unit id",
            "6 when a bound worker is hung",
            "5 when worker identity or terminal evidence is unknown, which is never success",
            "VERIFICATION NEVER GOES TO CODEX",
            "WHAT CODEX IS FOR",
        ] {
            assert!(text.contains(needle), "native text lacks {needle:?}");
        }
    }

    #[test]
    fn native_text_has_no_forwarder_spawn() {
        let text = collapsed(&native_text(&codex_only()));
        for forbidden in [
            "subagent_type",
            "codex-forward.sh",
            CODEX_FORWARD_SENTINEL,
            "1560000",
            "--resume-last",
            "--write --unit-id",
        ] {
            assert!(!text.contains(forbidden), "native text has {forbidden:?}");
        }
        assert!(text.contains("never spawn `loom-codex-forwarder`"));
    }

    #[test]
    fn native_text_interpolates_models_and_effort() {
        let text = native_text(&codex_only());
        for model in [
            CODEX_IMPLEMENTER_MODEL_TERRA,
            CODEX_IMPLEMENTER_MODEL_SOL,
            CODEX_IMPLEMENTER_MODEL_LUNA,
        ] {
            assert!(text.contains(&format!("--model {model}")), "{model}");
        }
        assert!(text.contains("--effort xhigh"));
    }

    #[test]
    fn native_text_constants_match_the_runtime() {
        assert_eq!(NATIVE_DEADLINE_SECS * 1000, 1_500_000);
        assert_eq!(MAX_ACTIVE_PER_STAGE, 6);
    }

    #[test]
    fn native_text_follows_the_shared_intro() {
        let mixed = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);
        assert!(native_text(&mixed).contains("PER SUBAGENT"));
        assert!(!native_text(&codex_only()).contains("PER SUBAGENT"));
    }
}
