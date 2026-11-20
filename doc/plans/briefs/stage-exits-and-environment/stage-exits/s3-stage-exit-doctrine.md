# S3: BLOCK-F, and every surface that must agree with it

Stage `stage-exits`, tier sonnet. Read `../common.md` first. Stage `completion-gates` has merged:
`loom stage dispute-criteria --field acceptance|wiring|wiring-tests` exists, and
`loom stage complete` labels failures `[criterion n]`, `[wiring n]` and `[wiring_tests n]`.

You own `loom/src/orchestrator/signals/{helpers.rs, tests_doctrine_v2.rs,
tests_doctrine_blocks.rs, tests_size.rs, v2_section_review.rs, format/sandbox_section.rs,
format/helpers.rs}`, `loom/src/commands/stage/{acceptance_runner.rs, complete_verification.rs,
dispute_transport.rs, dispute_criteria_tests.rs}`, `CLAUDE.md.template`,
`loom-hooks/commit-guard.sh`, `loom-hooks/_subagent-preamble.txt`,
`skills/loom-orchestration/SKILL.md` and `skills/loom-usage/SKILL.md`.

BLOCK-F is for the stage's MAIN agent: it is in the stage signal and the orchestration skill.
A subagent must never run `loom stage block` or `loom stage dispute-*`: a relayed block passes
the relay check with the parent session's environment (`commands/stage/state_relay.rs`), and
with stage-exits' S1 a block retires the main stage session mid-orchestration. Every surface a
subagent reads (`CLAUDE.md.template`, `_subagent-preamble.txt`) says the subagent reports the
need to its orchestrator instead.

## Why

Stage agents avoided disputes ("filing it ends the session"), reverted correct work to dodge one,
and ended turns asking the operator to act. Today's surfaces frame a dispute as a cost, tell an
agent to "STOP and report it as a blocker" without naming a command, and never mention
`loom stage block`.

## BLOCK-F, verbatim

Every copy is these exact bytes (one line, no line breaks inside):

    **When the stage cannot finish.** Fix what the stage can fix. A criterion, wiring check, contract, review finding or test-integrity event that is wrong gets a dispute: `loom stage dispute-criteria` (with `--field` for wiring and wiring-tests entries), `dispute-contract`, `dispute-findings` or `dispute-integrity`. Filing ends this session by design: the daemon starts a fresh session with the verdict, so waiting gains nothing. Never revert, weaken or postpone correct work to avoid a dispute. A need only a person can meet (a credential, a host install, a network domain or path the plan does not grant) gets `loom stage block <stage-id> "<what is needed and why>"`: the daemon retires the session and shows the reason to the operator. Commit your work before filing either. Never end a turn asking the operator to act while the stage is executing.

## 1. The stage signal

- `signals/helpers.rs`: a `const STAGE_EXIT_RULES: &str` holding BLOCK-F, pushed by
  `append_completion_rules` (about lines 165-170) after its bullet list, as its own paragraph
  followed by a blank line. `append_completion_rules` serves the standard, integration-verify and
  knowledge-distill prefixes (`cache.rs`); `cache.rs` is ledgered at 524 lines and is not
  edited. The knowledge prefix uses only `append_settled_completion_rules` and does not get it.
- `signals/tests_size.rs`: `generate_stable_prefix()` is 6,720 bytes and BLOCK-F (850 bytes)
  plus its paragraph break adds about 852, over the 7,168-byte ceiling. Raise `STABLE_PREFIX_MAX_BYTES` to `8_192` and extend its
  doc comment the way the BLOCK-D raise is documented: raised alongside BLOCK-F (stage exits),
  6,720 bytes before, the new actual after. The const line is not an assertion line.
- `signals/tests_doctrine_v2.rs`: add `const BLOCK_F` (the text above) and the test
  `block_f_agrees_across_every_surface` (exact name; acceptance runs it): the standard prefix
  (`super::cache::generate_stable_prefix()`), the integration-verify prefix
  (`super::cache::generate_integration_verify_stable_prefix()`), the knowledge-distill prefix
  (`super::cache::generate_knowledge_distill_stable_prefix()`) and `ORCHESTRATION_SKILL` each
  `contains(BLOCK_F)`, with a failure message like BLOCK-E's. Update the module doc comment.
- `signals/v2_section_review.rs::append_dispute_commands` (about lines 57-72): keep the
  batching advice (every dispute of one review round in one `dispute-findings` command) and
  replace "filing a dispute ends your turn and sends the stage to adjudication; loom retires
  your session when the verdict is applied, or when the dispute escalates to human review" with
  "each filing ends this session by design, and the daemon starts a fresh one with the
  verdict".
- `signals/format/sandbox_section.rs::format_missing_grants_note` (about lines 88-104): replace
  "STOP and report it as a blocker. The operator must create the path on the host and restart
  this stage's session." with "block the stage with `loom stage block <stage-id> \"<path> is
  missing on the host\"`: the operator creates the path and retries the stage." Change the test
  assertion at about line 134 to `assert!(content.contains("loom stage block"));` and add
  `assert!(!content.contains("report it as a blocker"));` (this file's inline tests are not a
  test file to the integrity gate; the retired-phrase sweep in `tests_doctrine.rs` never sees
  this text, so this assertion is its guard). `format_sandbox_section` is ledgered at 58 lines
  and is not touched.
- `signals/format/helpers.rs::append_package_cache_note` (about line 247): keep the
  `**Package-manager caches:**` opening (`tests_commit_timing.rs` matches it) and replace "STOP
  and report it as a blocker (it needs a plan-level `sandbox.filesystem.allow_write` entry); do
  not work around it." with "block the stage with `loom stage block <stage-id> \"<cache path>
  needs a plan-level sandbox.filesystem.allow_write entry\"`; do not work around it." Add an
  inline test in `format/helpers.rs`, `the_package_cache_note_names_loom_stage_block`, asserting
  the note contains `loom stage block` and not `report it as a blocker` (the doctrine sweep does
  not read this note either).
- `signals/tests_doctrine_blocks.rs::RETIRED_PHRASES`: add, split with `concat!` as the others
  are, with a comment naming BLOCK-F: `"report a needed sandbox block as a blocker"`,
  `"STOP and report it as a blocker"`, and `"ends your turn and sends the stage to adjudication;
  loom retires"`. Confirm BLOCK-F itself contains no retired phrase.

## 2. `loom stage complete`'s failure guidance

- `commands/stage/acceptance_runner.rs::print_acceptance_failure_guidance` (about lines
  211-231): keep "Fix the issues and run ..." first. Then: a check no correct implementation
  could pass gets a dispute, never a weakened fix, with the three forms

      loom stage dispute-criteria <id> --criterion-index <n> --reason "<why>"                       ([criterion n])
      loom stage dispute-criteria <id> --field wiring --criterion-index <n> --reason "<why>"        ([wiring n])
      loom stage dispute-criteria <id> --field wiring-tests --criterion-index <n> --reason "<why>"  ([wiring_tests n])

  (with the real stage id interpolated); commit first, and filing ends the session by design;
  a failure only a person can fix (a credential, a host install, a network domain or path the
  plan does not grant) gets `loom stage block <id> "<what is needed and why>"`. Keep the
  `--failure-output`, adjudicator and `--no-verify` sentences. `resolve_acceptance_dir` is
  ledgered (60 lines) and not touched. The file is 366 lines: the guidance function and its
  test may add at most 30, so it stays under 400. Factor the text into
  `fn failure_guidance(stage_id: &str) -> String`, which `print_acceptance_failure_guidance`
  prints, and add to the file's inline `mod tests` the test `failure_guidance_names_every_route`
  (exact name; acceptance runs it): the text for stage `s1` contains
  `loom stage dispute-criteria s1 --criterion-index`, `--field wiring --criterion-index`,
  `--field wiring-tests --criterion-index` and `loom stage block s1`.
- `commands/stage/complete_verification.rs::run_goal_checks` (about lines 57-84): before the
  final `bail!`, call `crate::commands::stage::acceptance_runner::print_acceptance_failure_guidance(checks.stage_id);`
  so a wiring or wiring-test failure shows the same routes.

## 3. A relayed dispute ends the turn

`commands/stage/dispute_transport.rs::send_via_relay` passes `false` as `end_turn` to
`context.emit`, so a sandboxed agent's relayed dispute notice never says "End your turn after the
confirmation." (a relayed block's does: `commands/stage/state_relay.rs`). Pass `true`. Test in
`dispute_criteria_tests.rs`: `relay_mode_dispute_ends_the_turn` (mirror
`state_relay.rs::relay_mode_writes_exactly_one_ticket_with_the_end_turn_reminder`).

## 4. Text surfaces

- `CLAUDE.md.template` Rule 13 (about line 181; anchor on the phrase): the template is pasted
  into every subagent's context, so the new text must not tell a subagent to block. The line
  ends "Confirm external dependencies exist; report a needed sandbox block as a blocker."
  Replace that final sentence with exactly (one line, the rest of the line unchanged):

      Confirm external dependencies exist. In a stage, the main agent blocks a need only a person can meet with `loom stage block <stage-id> "<what is needed and why>"` and disputes a wrong check; a subagent reports either to its orchestrator.

  The template is 18,734 bytes against a 20,480 cap (`tests_size.rs`); this adds about 180.
- `loom-hooks/_subagent-preamble.txt`: in the `SUBAGENT RESTRICTIONS` list, after the
  `loom stage complete` line, add
  `- NEVER run loom stage block or loom stage dispute-* - report the need to the main agent`.
  The preamble's first line and its BLOCK-A and BLOCK-D text are pinned by tests; this list is
  not. `fs::permissions::` (in acceptance) embeds the file.
- `loom-hooks/commit-guard.sh` (about line 629): "Do NOT end this session until all steps are
  complete." becomes "Do NOT end this session until all steps are complete, or a dispute or
  block is filed." Keep "LOOM WORKTREE COMPLETION CHECKLIST" (tests pin it).
- `skills/loom-orchestration/SKILL.md`: in the review loop (about lines 263-267) keep the
  batching advice and the 3-per-kind budget, and replace "Filing a dispute ends your turn and
  sends the stage to adjudication; loom retires your session when the verdict is applied, or
  when the dispute escalates to human review." with "Filing ends this session by design; the
  daemon starts a fresh session with the verdict." Add BLOCK-F verbatim as its own paragraph
  right after the "Review order (plan v2)" paragraph.
- `skills/loom-usage/SKILL.md` "Force Complete" (about lines 371-379): replace
  `# Request human review (preferred)` and the wrong
  `loom stage dispute-criteria <stage-id> "criteria X is incorrect because..."` with the real
  syntax and a comment that it routes to the adjudicator:

      # Dispute a wrong check; routes to the adjudicator, which can amend it
      loom stage dispute-criteria <stage-id> --criterion-index <n> --reason "<why it is wrong>"
      loom stage dispute-criteria <stage-id> --field wiring --criterion-index <n> --reason "<why>"

  and add a line for `loom stage block <stage-id> "<what is needed and why>"`. Keep the
  `--force-unsafe --assume-merged` lines. Acceptance checks that the wrong
  `dispute-criteria <stage-id> "criteria X` form is gone.

## Check

`cargo test --lib orchestrator::signals::tests_doctrine`, once. S1 and S2 edit the crate at the
same time: a compile error in a file you do not own is not yours (`common.md`). The main agent
also runs `bash ../scripts/check-hook-syntax.sh`, which parses every shell hook you edit.
