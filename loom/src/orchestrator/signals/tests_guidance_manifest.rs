//! Relocation manifest for `CLAUDE.md.template` and the orchestration skill.
//!
//! The installed template also serves sessions outside loom plans, so guidance
//! is relocated and never deleted. This file lists each relocated item
//! (template rules and removed orchestration-skill paragraphs) with its new
//! home and a literal needle, and freezes the template's `###` headings and
//! rule bullets as they stand at this plan's base.

use super::cache::generate_stable_prefix;
use crate::fs::permissions::constants::{HOOK_POST_TOOL_USE, HOOK_SESSION_START};

const TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");

/// Rule 14's verbose-output bullet. It is rewritten and relocated, so it may
/// leave the template; identified by its text, never by position.
const RULE_14_VERBOSE_BULLET: &str = r#"- Don't narrate between tool calls; explain decisions, not standard operations (Rule 19). Prefer Edit over Write. Pipe verbose output: tests through `tail -50` or `rg -B 2 -A 5 'FAIL|ERROR'`, builds to a file, linters through `head -30`. Skip agents for simple tasks. `.claudeignore` generated files, artifacts, lock files."#;

const FROZEN_LINES: &[&str] = &[
    r#"1. **PLANS** — plan files only under `doc/plans/`. (Rule 1)"#,
    r#"2. **MEMORY** — `loom memory`/`loom knowledge`, never Claude Code auto-memory. (Rule 12)"#,
    r#"3. **GIT** — only the main agent stages, commits, or completes a stage, and only as its final step. (Rule 4)"#,
    r#"4. **CONTEXT** — handoff only on the hook's ceiling report, never pre-emptively. (Rule 3)"#,
    r#"5. **BRANCHES** — never create one unasked. (Rule 9b)"#,
    r#"6. **DELEGATION** — a cost decision, tokens times model tier: beyond a small change, the main agent delegates. (Rule 7)"#,
    r#"### 1. PLANS"#,
    r#"### 2. NO PLACEHOLDERS"#,
    r#"### 3. CONTEXT CEILING = HANDOFF"#,
    r#"### 3b. POST-COMPACTION RECOVERY"#,
    r#"### 4. COMMIT AND COMPLETE (HOOK-ENFORCED)"#,
    r#"### 5. SUBAGENT RESTRICTIONS"#,
    r#"### 6. SUBAGENTS"#,
    r#"- Group small tasks into ONE subagent, never one per task or file; two subagents never write one file."#,
    r#"- Subagents are ONE-SHOT: brief completely, wait, harvest once, let it end. A follow-up is a fresh spawn of the same type."#,
    r#"- Never pass `name` to a subagent spawn (agent teams are the exception): a named spawn idles as a teammate instead of stopping, so `loom subagents watch` cannot bind it and a named `loom-code-reviewer` records no review round."#,
    r#"- Inside a loom stage, wait with one background `loom subagents watch`, never a poll loop. Outside a stage it exits 5 (it needs `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR`): a background Agent-tool subagent reports its own completion, so wait for that notification and never poll."#,
    r#"### 7. SKILLS, AGENTS, AND MODEL CHOICE"#,
    r#"### 8. NATIVE TOOLS AND CODE FORMATTING"#,
    r#"### 9. NO ATTRIBUTION"#,
    r#"### 9b. NEVER CREATE BRANCHES UNASKED"#,
    r#"### 10. WORKTREE ISOLATION"#,
    r#"### 11. BINARY USAGE"#,
    r#"### 12. PROJECT KNOWLEDGE"#,
    r#"1. Find the home in `INDEX.md`: the tier-2 topic whose blurb covers it, else a new `<category>/<slug>`. Tier-1 files take summaries only."#,
    r#"2. Write through the CLI; every write regenerates `INDEX.md`. `loom knowledge update <category>/<slug> "<entry>"` APPENDS, scaffolding a new topic. Correct a section with `loom knowledge replace-section <file> "<Heading>" "<body WITHOUT the ## line>"`, never `update`; on a non-matching heading it appends and SAYS so — read that line, or the stale text stands. Name the wrong claim in the replacement."#,
    r#"3. A new topic also gets `loom knowledge annotate <category>/<slug> --blurb "<80 chars max>"` and a 2-4 line summary plus link in its tier-1 file."#,
    r#"4. An Edit-tool fix leaves `INDEX.md` stale: run `loom knowledge sync` after it. End a batch with `loom knowledge check`."#,
    r#"### 13. SILENT FAILURE DETECTION"#,
    r#"### 14. TOKEN EFFICIENCY (HARD REQUIREMENT)"#,
    r#"- Query before reading (KNOWLEDGE-FIRST); open a file only once a lookup names its lines."#,
    r#"- Read ranges, not files (Read's offset/limit); whole files only when using all of it."#,
    r#"- Independent reads, greps, commands go in ONE message."#,
    r#"- Never re-read what's already in context, or from adjacent curiosity."#,
    RULE_14_VERBOSE_BULLET,
    r#"### 15. QUALITY GATES"#,
    r#"### 16. DEPENDENCIES"#,
    r#"### 17. CODE SIZE LIMITS"#,
    r#"### 18. KNOWLEDGE PLACEMENT"#,
    r#"### 19. WRITING STYLE"#,
];

struct Relocation {
    item: &'static str,
    frozen: Option<&'static str>,
    home: &'static str,
    needle: &'static str,
}

const TEMPLATE_HOME: &str = "CLAUDE.md.template";
const POST_TOOL_USE_HOME: &str = "loom-hooks/post-tool-use.sh";
const SESSION_START_HOME: &str = "loom-hooks/session-start.sh";
const PREFIX_HOME: &str = "standard stable prefix";

const RELOCATIONS: &[Relocation] = &[
    Relocation {
        item: "Rule 3 ceiling trigger",
        frozen: None,
        home: POST_TOOL_USE_HOME,
        needle: r#"CONTEXT CEILING REACHED:"#,
    },
    Relocation {
        item: "Rule 3 handoff command",
        frozen: None,
        home: POST_TOOL_USE_HOME,
        needle: r#"--trigger ceiling"#,
    },
    Relocation {
        item: "Rule 3 finish the unit, then hand off",
        frozen: None,
        home: PREFIX_HOME,
        needle: r#"finish the current unit and run `loom handoff --stage <id> --session <id> --trigger ceiling`"#,
    },
    Relocation {
        item: "Rule 3b re-read the signal",
        frozen: None,
        home: SESSION_START_HOME,
        needle: r#"Re-anchor: re-read ${SIGNAL_PATH}"#,
    },
    Relocation {
        item: "Rule 3b memory list",
        frozen: None,
        home: SESSION_START_HOME,
        needle: r#"loom memory list"#,
    },
    Relocation {
        item: "Rule 3b latest handoff",
        frozen: None,
        home: SESSION_START_HOME,
        needle: r#"/handoffs/ file"#,
    },
    Relocation {
        item: "Rule 14 verbose output",
        frozen: Some(RULE_14_VERBOSE_BULLET),
        home: TEMPLATE_HOME,
        needle: r#"2>&1; echo "exit=$?""#,
    },
    Relocation {
        item: "R1 loom plans on explicit request",
        frozen: None,
        home: TEMPLATE_HOME,
        needle: r#"**Loom plans:** only on explicit request"#,
    },
    Relocation {
        item: "R2 completion is the last act",
        frozen: None,
        home: PREFIX_HOME,
        needle: r#"is the LAST act of your session"#,
    },
    Relocation {
        item: "R2 post-completion work is lost",
        frozen: None,
        home: PREFIX_HOME,
        needle: r#"LOST WORK"#,
    },
    Relocation {
        item: "R3 criteria run through loom check",
        frozen: None,
        home: PREFIX_HOME,
        needle: r#"`loom check <stage-id>`"#,
    },
    Relocation {
        item: "R4 one commit per concern",
        frozen: None,
        home: TEMPLATE_HOME,
        needle: r#"one commit per concern"#,
    },
    Relocation {
        item: "R5 the subagent preamble file",
        frozen: None,
        home: TEMPLATE_HOME,
        needle: r#"_subagent-preamble.txt"#,
    },
    Relocation {
        item: "R10 fable override at spawn",
        frozen: None,
        home: TEMPLATE_HOME,
        needle: r#"Fable-tier work has no pinned agent type"#,
    },
    Relocation {
        item: "R11 when the stage cannot finish",
        frozen: None,
        home: PREFIX_HOME,
        needle: r#"**When the stage cannot finish.**"#,
    },
    Relocation {
        item: "R3 failed criteria are re-checked one by one",
        frozen: None,
        home: PREFIX_HOME,
        needle: r#"--criterion"#,
    },
];

fn home_text(home: &str) -> String {
    match home {
        TEMPLATE_HOME => TEMPLATE.to_string(),
        POST_TOOL_USE_HOME => HOOK_POST_TOOL_USE.to_string(),
        SESSION_START_HOME => HOOK_SESSION_START.to_string(),
        PREFIX_HOME => generate_stable_prefix(),
        other => panic!("unknown relocation home: {other}"),
    }
}

/// Frozen entries that are neither a whole template line nor the `frozen`
/// value of a relocation entry.
fn missing_frozen_lines(template: &str) -> Vec<&'static str> {
    FROZEN_LINES
        .iter()
        .copied()
        .filter(|frozen| {
            !template.lines().any(|l| l == *frozen)
                && !RELOCATIONS.iter().any(|r| r.frozen == Some(*frozen))
        })
        .collect()
}

fn missing_relocations(text_of: impl Fn(&str) -> String) -> Vec<String> {
    RELOCATIONS
        .iter()
        .filter(|r| !text_of(r.home).contains(r.needle))
        .map(|r| format!("{}: home `{}` lacks needle `{}`", r.item, r.home, r.needle))
        .collect()
}

#[test]
fn every_relocated_item_is_carried_by_its_home() {
    let missing = missing_relocations(home_text);
    assert!(
        missing.is_empty(),
        "relocated guidance missing from its home:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_frozen_template_line_survives_or_is_relocated() {
    let missing = missing_frozen_lines(TEMPLATE);
    assert!(
        missing.is_empty(),
        "template lines deleted without a recorded home:\n{}",
        missing.join("\n")
    );
}

#[test]
fn a_deleted_frozen_heading_fails_the_manifest_check() {
    let template = TEMPLATE.replace("### 3b. POST-COMPACTION RECOVERY", "");
    assert_eq!(
        missing_frozen_lines(&template),
        vec!["### 3b. POST-COMPACTION RECOVERY"]
    );
}

#[test]
fn the_writing_style_heading_is_still_mandatory() {
    let template = TEMPLATE.replace("### 19. WRITING STYLE", "");
    assert_eq!(
        missing_frozen_lines(&template),
        vec!["### 19. WRITING STYLE"]
    );
}

#[test]
fn replacing_the_rule_14_bullet_is_allowed() {
    let template = TEMPLATE.replace(RULE_14_VERBOSE_BULLET, "- replaced");
    assert!(missing_frozen_lines(&template).is_empty());
}

#[test]
fn a_removed_skill_instruction_home_fails_the_relocation_check() {
    let missing = missing_relocations(|home| {
        let text = home_text(home);
        if home == PREFIX_HOME {
            text.replace("LOST WORK", "")
        } else {
            text
        }
    });
    assert_eq!(missing.len(), 1, "expected exactly one miss: {missing:?}");
    assert!(
        missing[0].contains("R2 post-completion work is lost"),
        "wrong item named: {}",
        missing[0]
    );
}

#[test]
fn frozen_list_counts_the_base_template_shape() {
    assert_eq!(FROZEN_LINES.len(), 40);
    let headings = FROZEN_LINES.iter().filter(|l| l.starts_with("###")).count();
    assert_eq!(headings, 21);
}
