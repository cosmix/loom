# W7: doctrine for host-run verification (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first. Section 3.7 (exit codes, wait protocol) and section 4.5 (what each command prints) are the behaviour you describe. Paths are relative to `loom/` unless they start with `../`.

These texts are read by agents in every project loom runs. Describe loom's mechanism only. Never name a tool, path or config that only this repository has (`doc/loom/knowledge/mistakes/briefs-and-bug-reports.md`, "Loom's Own Repo Tooling Leaked Into a Generic Skill").

## Territory

You own:

- `../CLAUDE.md.template`, `../AGENTS.md.template`, `../README.md`;
- `../agents/loom-verifier.md`, `../agents/loom-software-engineer.md`;
- `../skills/loom-orchestration/SKILL.md`, `../skills/loom-plan-writer/SKILL.md`, `../skills/loom-plan-writer/references/*.md`, `../skills/loom-usage/SKILL.md`;
- `../loom-hooks/_subagent-preamble.txt`, `../loom-hooks/commit-guard.sh`;
- `src/orchestrator/signals/{helpers.rs, knowledge.rs, cache.rs, contract.rs}`, `src/orchestrator/signals/cache/blocks.rs` (`VERIFIER_GATE`), `src/orchestrator/signals/format/**`, `src/orchestrator/signals/tests_doctrine*.rs`;
- `src/orchestrator/adjudication/prompt/**`.

Edit only the sentences about completion, `loom check`, the contract freeze, where criteria run and the verifier's gate. Leave every other sentence alone.

## What plan 5 builds on (hard)

Plan 5 of the token-optimization programme (`doc/plans/token-optim/PLAN-05-guidance-and-gate-output.md`) runs after this plan and edits the same doctrine. It freezes and pins some of it:

- `../CLAUDE.md.template`: change no line that starts with `### `, `- ` or a number followed by `. ` (the hard stops, headings, rule bullets and steps plan 5 freezes), and add none. Rewrite Rule 4 and Rule 13 in their existing prose and code-block form.
- `VERIFIER_GATE` keeps these exact substrings:
  - `**The gate runs in a `loom-verifier`**`;
  - `` `loom check <stage-id>` ``;
  - `RUNNER-ENV`;
  - and gains `SESSION-ENV` (fact 5).
- `append_settled_completion_rules` (`signals/helpers.rs`) keeps `is the LAST act of your session` and `LOST WORK`.
- `../agents/loom-verifier.md`: add no fenced code block. Plan 5's contract reads the first fenced `bash` block it adds.
- `../skills/loom-orchestration/SKILL.md`: plan 5 deletes the paragraphs `**Complete ONLY a settled stage; ...**`, `**The gate in a standard stage.**` and `**When the stage cannot finish.**` in favour of their copies in the stable prefix. Rewrite them here anyway, so the skill is true until then, and keep every fact they carry also present in the stable-prefix copy (`append_settled_completion_rules`, `VERIFIER_GATE`, `STAGE_EXIT_RULES`).

## Size limits (hard)

`src/orchestrator/signals/tests_size.rs` caps the doctrine sessions receive:

- `CLAUDE_MD_TEMPLATE_MAX_BYTES` is 20,480; the template is 20,320 bytes at `fb927c6e`.
- `STABLE_PREFIX_MAX_BYTES` is 8,192.
- `STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` is 10,240, with tens of bytes of headroom.

Plan 5 of the token-optimization programme measures and lowers these later. Therefore:

- Every replacement in `../CLAUDE.md.template` and in signal text (`signals/helpers.rs`, `signals/format/**`, `knowledge.rs`, `cache.rs`, `contract.rs`) is no longer than the text it replaces. Check with `wc -c` before and after, and report both numbers per file.
- Never change a ceiling constant.
- If a fact cannot fit, shorten the surrounding completion text it replaces, never other rules.
- `../skills/loom-orchestration/SKILL.md` has no ceiling, but treat it the same way: net growth at most a few hundred bytes, reported.

## The new facts to state

1. **Where criteria run.**
   - Every acceptance, setup, wiring, truth, dead-code, contract and impact-test command runs in a host job the loom daemon starts. It runs under loom's confinement: bubblewrap on Linux, `sandbox-exec` on macOS; no network; private `/tmp` and `/var/tmp`; read-only git metadata.
   - Never in the session's sandbox.
   - Timeouts are unchanged: 300 s per string criterion, `timeout_secs` on the extended form.
2. **`loom stage complete <stage>` in a session.**
   - It asks the daemon to verify the committed tree. Exit 3 means requested or still running: run the same command again in the next Bash call.
   - Exit 0 means the daemon completed the stage: end the session.
   - Exit 1 means a criterion or check failed (the failing outputs are printed) or the tree is not committed: fix, commit, run again.
   - Exit 4 means the run was cancelled or interrupted: run again.
   - It is still the session's last act, and the agent does nothing else while it waits.
3. **`loom check <stage>` in a session** (the verifier, any agent, every judge) follows the same exit codes and verifies the tree as it is, committed or not.
4. **The contract freeze** relays and ends the writer's turn. The daemon runs the contracts on the host, then freezes and starts the implementation session, or parks the stage with the refusal.
5. **The verifier's gate.** `loom check <stage>` is authoritative for the stage's criteria.
   - A criterion that passes there but fails in the verifier's own shell is environment, not a defect. Report it as SESSION-ENV. It is not a FAIL; nothing is disputed.
   - A criterion that fails under `loom check` is a FAIL, whatever the shell shows.
   - The RUNNER-ENV dispute route stays for a criterion that fails under `loom check` only because of the confined runner (no network, timeout, git metadata). Keep that wording.
6. **Disputes** for "fails only because of where completion runs" stay as they are; the runner is now always the host's.
7. **Gone:**
   - the evidence record, the broker and the "verification passed but not completed yet" notice;
   - the claim that acceptance runs inside the stage's sandbox;
   - macOS's inherited Seatbelt and its socket;
   - the "pass cache does not work in a session" caveat: host runs write the acceptance cache.

## Where (re-locate each with `rg`; numbers measured at `145d32ac`)

- `../CLAUDE.md.template` Rule 4 (the commit/complete block, about lines 77-90) and Rule 13 (about line 185). `../AGENTS.md.template` about line 60.
- `src/orchestrator/signals/helpers.rs`:
  - about line 150 ("Then run `loom stage complete`");
  - `append_settled_completion_rules` (about 208-216);
  - `STAGE_EXIT_RULES` (about 221);
  - about 228 ("from the worktree ROOT").
- `format/helpers.rs` about 241 (stage end sequence); `format/sections.rs` about 360; `knowledge.rs` about 124; `cache.rs` about 270; `contract.rs` about 248 (the writer's freeze step); `format/v2_section.rs` about 65; `format/v2_section_review.rs` about 34 and 75-80; `format/codex.rs` about 172-181 (BLAST RADIUS: codex runs `--sandbox danger-full-access` wherever the outer sandbox refuses nesting, Linux included).
- `tests_doctrine*.rs`: these pin text across signal, template and hook byte for byte. Change every copy together and update the pins. Assertion edits here raise `TI-edit` events that the main agent disputes. Do not weaken a pin; re-point it at the new text.
- `../skills/loom-orchestration/SKILL.md`:
  - about 50-56 and 69-70 (the verifier runs criteria through `loom check`, which is the host runner, with SESSION-ENV as in fact 5);
  - about 183 and 294 (stage exit rules);
  - about 304-318 ("What `loom stage complete` checks, in order": same checks, run by the daemon's host job, review gate and integrity checked by the daemon at apply).
- `../skills/loom-plan-writer/SKILL.md`:
  - about 347 ("Acceptance runs INSIDE the stage's sandbox": rewrite to the host job, keeping the rule that a criterion needing something the confined runner cannot grant is not a criterion);
  - about 366-368.
- `references/sandbox.md`:
  - the "Acceptance runs INSIDE the stage's sandbox — verify it THERE" section, about 9-16: becomes "Criteria run in the daemon's host job", with "verify a criterion with `loom check <stage>` from a session or the host, not by running it bare";
  - about 110-111 (`setup:`);
  - "Where criteria run", about 125-143.
- `references/grounding-protocols.md` about 116; `verification-rules.md` about 19 and 55; `v2-contracts.md` about 92, 114 and 129; `authoring-detail.md` about 131; `../skills/loom-usage/SKILL.md` about 266, 685 and 719.
- `../agents/loom-verifier.md` about 25 and 34; `../agents/loom-software-engineer.md` about 61.
- `../loom-hooks/_subagent-preamble.txt` about 5-6.
- `../loom-hooks/commit-guard.sh` about 545, 569 and 579-583: the checklist says to run `loom stage complete` until it exits 0, 1 or 4.
- `src/orchestrator/adjudication/prompt/criterion.rs`:
  - step 1 (about 101-136): `loom check <stage> --no-cache` relays to the daemon; re-run on exit 3; the printed outputs are the host runner's;
  - `where_completion_runs` (about 191-209): the runner is the daemon's host job.
  - Update `prompt/tests.rs` and the golden `prompt/tests_golden.rs` to the new text.
- `../README.md`: the confinement section (about line 1099: "Linux always builds its own", "`inherit` … the stage session's, for `loom stage complete`"), and wherever it describes completion, the broker or macOS inheritance (`rg -n -i 'broker|inherit|evidence record|completion' ../README.md`).

Keep every edited markdown file lint-clean under the repository's markdownlint config. Every fenced block names a language. No LLMisms (CLAUDE.md Rule 19).

## Proof (run once, from `loom/`)

```bash
cargo test --lib orchestrator::signals:: orchestrator::adjudication:: 2>&1 | tail -30
cd .. && bash loom-hooks/tests/run-all.sh 2>&1 | tail -10
rg -n -i 'inside the stage.s sandbox|LOOM_CONTROL_EVIDENCE|completion broker|ValidateInherited|not completed yet' CLAUDE.md.template AGENTS.md.template README.md agents skills loom-hooks loom/src/orchestrator/signals loom/src/orchestrator/adjudication
```

The last command must print nothing.

## Report

Every file changed with the sentence-level change in one line each; pins re-pointed; anything you could not place.
