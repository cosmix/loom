# H1 — skill-trigger matcher

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first.

## Goal

Skill suggestions fire on human prompts only, need real evidence, and appear once per session.
Evidence: report section 4.4 — 2,222 injections (about 349k tokens), conversion near zero;
`loom-ci-cd` suggested 934 times on the word `stage`; suggestions fire on agent-to-agent
messages.

## Files you own (write)

- `loom-hooks/skill-trigger.sh`
- `loom/src/commands/skill_index.rs`
- `loom/tests/integration/hooks_skill_trigger.rs`, `hooks_skill_trigger_codex.rs`

Read-only: `loom-hooks/_read_ledger.sh`, `_read_discipline.sh` (another worker owns them; you
reuse their ledger helpers by sourcing, as other hooks do),
`loom/src/commands/hook/user_prompt.rs:334-338` (`is_machine_generated`, the rule to mirror).

## Where things are

`skill-trigger.sh` embeds a Python matcher: `STOPWORDS` (25-33), `MIN_SCORE = 2`,
`MAX_SUGGESTIONS = 5` (22-23), `_score_keywords` (102-109: a phrase or a name match scores 2, a
single word 1), `_add_project_matches` (155-170: repo-type marker adds 1), `_rank` (173-179),
`_render` (219-234), `--codex` rendering in `_render_one` (200-216). It keeps no state.
`skill_index.rs:34` holds a second stopword list used when SKILL.md triggers are indexed
(`is_stopword`, 263-267; the comment at 272 says it mirrors the shell list).

## Steps

1. Machine-generated prompts. Return no output when the prompt, after leading whitespace, starts
   with `<`, `Background agent` or `Caveat:` — the same three tests as `is_machine_generated`.
2. Evidence rule. A skill qualifies only when it has (a) a phrase hit or a name match, or (b) two
   distinct single-word keyword hits. The repo-type marker never counts toward (a) or (b); it
   only orders qualified skills. This replaces "score >= MIN_SCORE" as the gate.
3. Loom vocabulary. Add to both stopword lists: `stage`, `job`, `state`, `result`, `backend`,
   `event`, `hook`, `option`, `session`, `token`, `context`, `sync`, `plan`, `agent`, `model`,
   `report`, `graph`, `document`, `prompt`, `review`, `comment`. Keep the two lists identical and
   add a Rust test that reads `loom-hooks/skill-trigger.sh` and asserts the sets are equal, so
   they cannot drift again.
4. Once per session. Record suggested skill names in a session ledger using the pattern of
   `_loom_ledger_file` (`_read_discipline.sh:135-154`: under
   `$LOOM_WORK_DIR/hooks/<kind>/<session>/` in a stage, `$TMPDIR/loom-<kind>/<sid>.tsv`
   outside one) with kind `skills`. A skill already in the ledger is not suggested again. When
   nothing new qualifies, print nothing.
5. Pin the header. The current advisory header (the `keyword hits shown; repo: markers are
   context, not a reason to load` sentence) is the only one allowed; two directive variants
   shipped in September. Add an integration test asserting the exact header and asserting the
   output never contains `Load EVERY`.

## Traps

- `doc/loom/knowledge/mistakes/hooks-shell-portability.md` records a past ranking bug where
  `type` and `error` were removed from the stopwords; read that heading before editing the list.
- The hook must exit 0 and print nothing on any internal error, including an unwritable ledger.
- Tests run through `cargo test --test integration`; this hook has no `run-all.sh` entry.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --test integration hooks_skill_trigger`
— run once. New cases: agent-message prompt silent; `stage` alone on a repo with the ci-cd marker
silent; `model selection` alone still qualifies `loom-model-evaluation` only through the phrase
rule — decide with a test whether that phrase trigger should stay and report it (the SKILL.md
files belong to the doctrine stage); second identical prompt in one session silent.
