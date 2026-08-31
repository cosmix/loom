//! Cross-surface consistency tests for the shared agent-doctrine blocks.
//!
//! Two doctrines are duplicated across surfaces owned by DIFFERENT stages, in
//! DIFFERENT languages, so no compiler or linter relates them: BLOCK-A (the
//! no-verify rule) appears in the signal prefixes from `cache.rs`, in
//! `CLAUDE.md.template` (Rule 5 and worker preambles), and in the stderr of
//! `hooks/subagent-verify-guard.sh`; BLOCK-B (the model playbook) appears in
//! `CLAUDE.md.template` (Rule 7) and `skills/loom-plan-writer/SKILL.md`. If
//! the copies drift, one surface teaches a rule the others contradict, and a
//! subagent obeying the wrong copy is blocked by the hook with no allowed
//! alternative.
//!
//! A third, BLOCK-C (subagent-waiting), is pinned the same way in the sibling
//! `tests_doctrine_waiting.rs`; the emitted STABLE PREFIX / SIGNAL text (rather
//! than the static guidance surfaces BLOCK-A and BLOCK-B live on) is pinned in
//! `tests_doctrine_prefixes.rs` instead - both splits keep files under budget.
//!
//! A fourth, BLOCK-D (the subagent context-ceiling rule), is pinned HERE
//! rather than split out: it appears on both a static surface
//! (`CLAUDE.md.template` Rule 5) and the emitted signal (`cache.rs`'s
//! `append_subagent_ceiling_block`), unlike BLOCK-A/B (static-only, pinned in
//! this file) or BLOCK-C (static-only, pinned in the sibling). It exists
//! because a subagent that never sees the PostToolUse hook's literal
//! `SUBAGENT CEILING REACHED` report has no falsifiable way to know it has
//! NOT reached its ceiling, and was observed confabulating one from CLAUDE.md
//! prose alone (five subagents, zero files written, real usage 34k-71k
//! against a 120,000 ceiling - see `block_d_agrees_across_every_surface`).
//!
//! The two singular surfaces are embedded with `include_str!`, so moving either is a
//! COMPILE error rather than a silently-skipped test; the `agents/` roster is scanned
//! instead, because it grows. BLOCK-A's ABSENCE from the knowledge/knowledge-distill
//! prefixes is pinned by `cache.rs`'s own unit tests, not here.

use std::fs;
use std::path::{Path, PathBuf};

use super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_knowledge_stable_prefix, generate_stable_prefix,
};
use super::format::format_codex_implementers_section;
use crate::fs::permissions::constants::{HOOK_CODEX_FORWARD, HOOK_SUBAGENT_VERIFY_GUARD};
use crate::models::stage::{Implementer, Implementers};

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");
const PLAN_WRITER_SKILL: &str = include_str!("../../../../skills/loom-plan-writer/SKILL.md");

/// Repo root, resolved from the crate directory at compile time.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("loom/ always has a parent - the repo root")
        .to_path_buf()
}

/// Every `agents/*.md` file, read at run time.
///
/// Deliberately a DIRECTORY SCAN rather than a hand-written `include_str!` list:
/// the agent roster grows (this plan alone added `loom-advisor.md`), and a list
/// maintained by hand would quietly stop covering the newest file - which is
/// exactly the surface most likely to be written from a stale template.
fn agent_definitions() -> Vec<(String, String)> {
    let dir = repo_root().join("agents");
    let mut found: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("readable dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| {
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            (
                format!("agents/{}", path.file_name().unwrap().to_string_lossy()),
                text,
            )
        })
        .collect();
    found.sort();

    assert!(
        !found.is_empty(),
        "no agent definitions found in {} - a directory scan that silently \
         matches nothing is a test that silently checks nothing",
        dir.display()
    );
    found
}

#[path = "tests_doctrine_blocks.rs"]
mod blocks;
use blocks::{BLOCK_A, BLOCK_B, BLOCK_D, RETIRED_PHRASES};

/// Every static guidance surface pasted into a subagent prompt, as (label, text).
fn guidance_surfaces() -> Vec<(String, String)> {
    let mut surfaces = vec![
        (
            "CLAUDE.md.template".to_string(),
            CLAUDE_MD_TEMPLATE.to_string(),
        ),
        (
            "skills/loom-plan-writer/SKILL.md".to_string(),
            PLAN_WRITER_SKILL.to_string(),
        ),
    ];
    surfaces.extend(agent_definitions());
    surfaces
}

#[test]
fn block_a_agrees_across_every_surface() {
    let signal_prefix = generate_stable_prefix();
    let iv_prefix = generate_integration_verify_stable_prefix();

    for (label, text) in [
        ("signal stable prefix", signal_prefix.as_str()),
        ("signal integration-verify prefix", iv_prefix.as_str()),
        ("CLAUDE.md.template", CLAUDE_MD_TEMPLATE),
        ("hooks/subagent-verify-guard.sh", HOOK_SUBAGENT_VERIFY_GUARD),
    ] {
        assert!(
            text.contains(BLOCK_A),
            "{label} does not carry BLOCK-A verbatim. The no-verify rule must be \
             byte-identical on every surface an agent reads it from; reword one \
             and you must reword all of them. Expected to find:\n{BLOCK_A}"
        );
    }
}

#[test]
fn block_b_agrees_across_every_surface() {
    for (label, text) in [
        ("CLAUDE.md.template", CLAUDE_MD_TEMPLATE),
        ("skills/loom-plan-writer/SKILL.md", PLAN_WRITER_SKILL),
    ] {
        assert!(
            text.contains(BLOCK_B),
            "{label} does not carry BLOCK-B verbatim. The model playbook must be \
             byte-identical wherever it appears. Expected to find:\n{BLOCK_B}"
        );
    }

    // BLOCK-B names the codex lane's model and effort as prose literals, so
    // equality with the surfaces above is not enough: editing the constants in
    // codex.rs would leave every surface agreeing on a RETIRED model and this
    // test still green. Pin the prose to the constants the signal interpolates.
    assert!(
        BLOCK_B.contains(crate::codex::CODEX_IMPLEMENTER_MODEL_TERRA),
        "BLOCK-B must name CODEX_IMPLEMENTER_MODEL_TERRA ({}); update every surface \
         when the constant changes",
        crate::codex::CODEX_IMPLEMENTER_MODEL_TERRA
    );
    assert!(
        BLOCK_B.contains(crate::codex::CODEX_IMPLEMENTER_MODEL_LUNA),
        "BLOCK-B must name CODEX_IMPLEMENTER_MODEL_LUNA ({}); update every surface \
         when the constant changes",
        crate::codex::CODEX_IMPLEMENTER_MODEL_LUNA
    );
    assert!(
        BLOCK_B.contains(crate::codex::CODEX_IMPLEMENTER_EFFORT),
        "BLOCK-B must name CODEX_IMPLEMENTER_EFFORT ({}); update every surface \
         when the constant changes",
        crate::codex::CODEX_IMPLEMENTER_EFFORT
    );
}

#[test]
fn block_d_agrees_across_every_surface() {
    let signal_prefix = generate_stable_prefix();
    let iv_prefix = generate_integration_verify_stable_prefix();

    for (label, text) in [
        ("signal stable prefix", signal_prefix.as_str()),
        ("signal integration-verify prefix", iv_prefix.as_str()),
        ("CLAUDE.md.template", CLAUDE_MD_TEMPLATE),
    ] {
        assert!(
            text.contains(BLOCK_D),
            "{label} does not carry BLOCK-D verbatim. The subagent \
             context-ceiling rule must be byte-identical on every surface a \
             subagent reads it from. Expected to find:\n{BLOCK_D}"
        );
    }

    // The literal hook string, spelled exactly as `hooks/post-tool-use.sh`
    // emits it - a paraphrase here would leave a subagent unable to
    // recognize the ONE signal that means it has actually reached the
    // ceiling.
    assert!(
        BLOCK_D.contains("SUBAGENT CEILING REACHED:"),
        "BLOCK-D must quote the hook's literal `SUBAGENT CEILING REACHED:` \
         line verbatim, or a subagent has no falsifiable way to tell a real \
         ceiling report from its own inference"
    );

    // Documentation stages (knowledge, knowledge-distill) never spawn
    // subagents, so BLOCK-D must not leak into their prefixes.
    let knowledge_prefix = generate_knowledge_stable_prefix();
    let distill_prefix = generate_knowledge_distill_stable_prefix();
    for (label, text) in [
        ("signal knowledge prefix", knowledge_prefix.as_str()),
        ("signal knowledge-distill prefix", distill_prefix.as_str()),
    ] {
        assert!(
            !text.contains(BLOCK_D),
            "{label} must not carry BLOCK-D: this stage type runs single-agent \
             with no subagents spawned"
        );
    }
}

#[test]
fn codex_forward_sentinel_agrees_across_surfaces() {
    use crate::codex::CODEX_FORWARD_SENTINEL;
    use crate::fs::permissions::constants::HOOK_CODEX_FORWARD_GUARD;

    // The hook enforces exactly the token the signal doctrine mandates. The
    // signal side is pinned in tests_cache.rs (the generated section contains
    // the constant); this side pins the shell literal to the same constant.
    assert!(
        HOOK_CODEX_FORWARD_GUARD.contains(CODEX_FORWARD_SENTINEL),
        "hooks/codex-forward-guard.sh must grep for CODEX_FORWARD_SENTINEL \
         ({CODEX_FORWARD_SENTINEL}); a hook keyed on a drifted token enforces \
         nothing"
    );

    let (_, forwarder) = agent_definitions()
        .into_iter()
        .find(|(label, _)| label == "agents/loom-codex-forwarder.md")
        .expect(
            "agents/loom-codex-forwarder.md must exist - the signal doctrine \
             spawns codex work by that agent type",
        );
    for needle in [CODEX_FORWARD_SENTINEL, "codex-forward.sh"] {
        assert!(
            forwarder.contains(needle),
            "agents/loom-codex-forwarder.md must mention {needle:?} - the \
             forwarder contract, the hook, and the signal doctrine describe \
             one protocol"
        );
    }

    // The playbook surfaces route codex work through the forwarder, never a
    // direct spawn of the plugin wrapper (whose tools restriction is not
    // enforced on this spawn path - observed implementing instead of
    // forwarding, 2026-08-07).
    for (label, text) in [
        ("CLAUDE.md.template", CLAUDE_MD_TEMPLATE),
        ("skills/loom-plan-writer/SKILL.md", PLAN_WRITER_SKILL),
    ] {
        assert!(
            text.contains("loom-codex-forwarder"),
            "{label} must route codex work through loom-codex-forwarder"
        );
    }
}

/// Pins the forwarding wrapper's navigation-kit preamble: the WRAPPER half of
/// the pair completed by [`codex_navigation_kit_signal_doctrine_names_the_kit`].
/// Three things are pinned: the preamble TEXT (needles below), the
/// COMPOSITION that splices it onto the caller's prompt, and the HAND-OFF
/// that passes the composed `$task` - not the bare `$prompt` - to the
/// companion runtime. A wrapper can keep every word of the preamble while
/// never delivering it (e.g. reverting to `task "$prompt"`), so text needles
/// alone are not enough; if any of the three broke, codex would silently fall
/// back to sweeping the whole knowledge base again with nothing in CI to say so.
#[test]
fn codex_navigation_kit_wrapper_carries_and_delivers_the_preamble() {
    for needle in [
        "loom map --find-all",
        "loom map --outline",
        "loom map --impact",
        "loom knowledge context",
        "NEVER run git",
        ".work/",
    ] {
        assert!(
            HOOK_CODEX_FORWARD.contains(needle),
            "hooks/codex-forward.sh must still carry {needle:?} - the signal \
             doctrine tells the orchestrator this navigation kit and these \
             prohibitions already reach every codex prompt, so the wrapper \
             dropping any of them would leave that promise false"
        );
    }

    // The needles above pin the preamble TEXT only. A wrapper can keep every
    // word of it and still never deliver it: the composition that splices the
    // preamble onto the caller's prompt, and the hand-off that passes the
    // composed value - not the bare prompt - to the companion runtime, are
    // separate lines neither needle touches. Pin those too.
    assert!(
        HOOK_CODEX_FORWARD.contains("task=\"${preamble}"),
        "hooks/codex-forward.sh must still compose the preamble onto the task \
         via `task=\"${{preamble}}...` - the navigation kit only reaches codex \
         if the wrapper actually splices it onto the caller's prompt, not \
         merely if the preamble text still sits in the file"
    );
    assert!(
        HOOK_CODEX_FORWARD.contains("task \"$task\""),
        "hooks/codex-forward.sh must still hand the COMPOSED `$task` - not the \
         bare `$prompt` - to the companion runtime: reverting `task \"$task\"` \
         to `task \"$prompt\"` silently drops the navigation kit from every \
         codex prompt while the preamble text stays in the file and every \
         needle above still matches"
    );
}

/// Pins the signal doctrine's SIGNAL half of the same pair: the text told to
/// the orchestrator, not the wrapper it describes (that half is
/// [`codex_navigation_kit_wrapper_carries_and_delivers_the_preamble`]). The
/// signal doctrine tells the orchestrator that codex arrives at every prompt
/// already carrying `loom map`/`loom knowledge context` anchors and the
/// instruction not to read CLAUDE.md or sweep doc/loom/knowledge/ - so the
/// orchestrator never repeats any of that itself. If this doctrine text
/// stopped naming the kit, an orchestrator reading the signal would have no
/// way to know the wrapper already supplies it, and would start re-pasting it.
#[test]
fn codex_navigation_kit_signal_doctrine_names_the_kit() {
    let implementers = Implementers::new(vec![Implementer::Codex]);
    let content = format_codex_implementers_section(&implementers, true);

    for needle in ["loom map --find-all", "loom map --impact", "AGENTS.md"] {
        assert!(
            content.contains(needle),
            "the codex-implementers signal section must reference {needle:?} - \
             it tells the orchestrator the wrapper already equips codex with \
             the navigation kit, so the doctrine text must name the kit it is \
             pointing to, not just assert one exists"
        );
    }
}

#[test]
fn no_guidance_surface_still_tells_a_subagent_to_verify() {
    let mut surfaces = guidance_surfaces();
    surfaces.push(("signal stable prefix".to_string(), generate_stable_prefix()));
    surfaces.push((
        "signal integration-verify prefix".to_string(),
        generate_integration_verify_stable_prefix(),
    ));
    surfaces.push((
        "signal knowledge prefix".to_string(),
        generate_knowledge_stable_prefix(),
    ));
    surfaces.push((
        "signal knowledge-distill prefix".to_string(),
        generate_knowledge_distill_stable_prefix(),
    ));

    let mut leftovers = Vec::new();
    for (label, text) in surfaces {
        for phrase in RETIRED_PHRASES {
            if text.contains(phrase) {
                leftovers.push(format!("{label} still contains {phrase:?}"));
            }
        }
    }

    assert!(
        leftovers.is_empty(),
        "guidance surfaces still carry phrasing the no-verify rule retired, so a \
         subagent obeying them would be blocked by hooks/subagent-verify-guard.sh \
         with no allowed alternative:\n  {}",
        leftovers.join("\n  ")
    );
}

// STABLE-PREFIX / emitted-signal doctrine tests (settled-completion, the subagent-budget
// cadence framing, KNOWLEDGE_CONSUMPTION_CONTRACT) live in tests_doctrine_prefixes.rs;
// brief-rendering in tests_brief.rs.
