# live-checks R4: plan-writer doctrine for the live profile

Working dir `loom/` (doctrine files are reached as `../skills/...`). Read
`doc/plans/briefs/live-test-lane/common.md` first. No contract covers your files; the stage's
review does.

Files (yours alone): `../skills/loom-plan-writer/SKILL.md`,
`../skills/loom-plan-writer/references/sandbox.md`,
`../skills/loom-plan-writer/references/verification-rules.md`,
`../skills/loom-plan-writer/references/v2-contracts.md`. Plans 08-11 edited `SKILL.md`: read the
current text before editing.

## Edits

1. `SKILL.md` Section 8, environment inventory, "Unix sockets" bullet: it describes the session
   sandbox, and criteria do not run there. State both: a stage SESSION's sandbox denies
   `AF_UNIX` (the remedies stay: a TCP-only option, or `allow_all_unix_sockets` with
   `--accept-unix-socket-risk`), while criteria run in loom's confined runner, which on Linux
   permits Unix sockets under its private `/tmp` and loopback TCP (on macOS its Seatbelt profile
   denies both). Replace "or the command moved to an operator-gated stage" with "or, for a test
   that drives a real Claude Code (network and a credential), a `live:` entry on the stage".
   Add one inventory bullet: "Live entries: run only by `loom check --live <stage>`, inside the
   live profile; they need `loom live credential set` on the machine that runs the plan, and
   `loom live status` names anything missing. A required entry without a passing run for the
   current content fails `loom stage complete` with `run loom check --live <stage>`. The stage
   description says when its session runs `loom check --live`: after the final review round's
   fixes and before the commit, since any later change to the tree's content makes the receipt
   stale, and an advisory entry runs only when the session asks for it."
2. `SKILL.md` Section 6 verification-fields table, a `live` row (v2): proves behaviour that needs
   a real Claude Code through the gate; fields `command`, `timeout_secs` (60-7200, default
   1800), `hosts` (default and only value `api.anthropic.com`), `credential` (`anthropic`),
   `required` (default false); standard and integration-verify stages only; never run by
   completion. The YAML of the plan that INTRODUCES a stage field cannot use it: the installed
   loom parses the plan at `loom init`. Add to the plan-version lints bullet that plan-authored
   commands using `--include-ignored`, `LOOM_TEST_REQUIRE_SANDBOX_FREE`, `LOOM_LIVE_PROFILE`,
   `dangerouslyDisableSandbox`, `--dangerously-skip-permissions`,
   `--dangerously-bypass-approvals-and-sandbox`, `--yolo` or `--sandbox danger-full-access` are
   errors (`--include-ignored` allowed in a `live:` command), and that
   `command_confinement: inherit` and `allow_unsandboxed_escape` do not exist.
3. `references/sandbox.md`: in "The ungrantable-resource rule" name the `live:` entry as the
   route for a test that needs network with a credential; the real `HOME`, a host daemon or
   `/run` stay ungrantable everywhere (the live profile gives a private `HOME`, never the real
   one). Rewrite the "Registry domains" paragraph to: every criterion is confined, so a registry
   fetch in one is an error whatever the domains say; the remedy is a `provision` entry. Rewrite
   the AF_UNIX paragraph the same way as edit 1 (session sandbox versus confined runner).
4. `references/verification-rules.md` rule 4 ("Environment-dependent tests"): a probe for a
   facility the environment advertises in `LOOM_TEST_FACILITIES` fails instead of skipping
   (loom's runner advertises what it provides); a test that drives a real Claude Code is
   `#[ignore = "loom-live-profile: ..."]`, guards itself with `require_live_profile`, skips
   elsewhere with a visible `loom-live-profile: skip` line, and runs through a `live:` entry.
5. `references/v2-contracts.md` Section 5 table: a "Live entries" row (stages `standard`,
   `integration-verify`; fails when a `required` entry has no passing outcome in a receipt for
   the current content; resolved by `loom check --live <stage>`, by fixing the code or the
   command, by `loom stage dispute-criteria <stage> --field live --criterion-index <n>`, or, for
   a missing credential only, by `loom stage block`). In the "Disputes" bullet add `--field
   live`.
6. Sweep the four files for any remaining text that tells an agent to ask for, or run, a test on
   the host outside loom's confinement (`rg -n -i 'operator-gated|outside the sandbox|on the
   host'` and read each hit); provision's host execution is a different, correct statement.

## Traps

- Doctrine states current truth only; no "previously" or "used to".
- These files are repository copies; the installed copies change when the operator reinstalls.
- No formatter runs over Markdown here; keep tables aligned by hand.

## Report

Files changed and each hit of the step 6 sweep with what you did about it.
