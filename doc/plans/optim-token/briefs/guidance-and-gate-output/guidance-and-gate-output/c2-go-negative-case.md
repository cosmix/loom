# C2: real-index fixture and the Go-on-Rust negative case (codex unit `skill-trigger-negative`)

Lane: codex, `--model gpt-5.6-terra --effort xhigh`, forwarded by `loom-codex-forwarder` in wave 1, before any Claude worker starts. Paths below are repo-relative; cargo runs from `loom/`.

## Files

Write:

- `loom/tests/integration/hooks_skill_trigger_fixture.rs` (one new method on `FakeHome`)
- `loom/tests/integration/hooks_skill_trigger_evidence.rs` (one new test)

Read only: `skills/loom-golang/SKILL.md` (its `triggers:` list), `loom/src/commands/skill_index/install.rs` (`execute_in_agent_dir`), `loom/src/commands/skill_index.rs` (`build_index`, `STOPWORDS`), `loom/src/skills/index_catalog.rs` (`CATALOG_DIR_NAME`, `catalog_dir_for`), `loom-hooks/skill-trigger.sh` (`_score_keywords`, `_rank`).

Never run git and never write under `.loom/`. Do not edit `skills/loom-golang/SKILL.md`: a Claude worker replaces its trigger list after you, and the test you write must fail until then.

## Why

`_rank` in `loom-hooks/skill-trigger.sh` qualifies a skill on two single-word hits (score 1 each) or one phrase hit (score 2, `_score_keywords`). `skills/loom-golang/SKILL.md` lists bare `struct` and `map`, so a Rust prompt with both words recommends the Go skill. The fixture's keyword index is hand-written (`write_skill_index`), so no existing test sees a real `SKILL.md`.

## Step 1: `FakeHome::index_real_skill` in `hooks_skill_trigger_fixture.rs`

Add to `impl FakeHome`, directly after `add_index_entry`:

```rust
/// Replace the hand-written index with the one the real indexer builds after
/// the repository's own `skills/<name>/SKILL.md` is copied into the fake
/// catalog, so a test sees that skill's real triggers.
pub fn index_real_skill(&self, name: &str)
```

Body:

1. Source: `Path::new(env!("CARGO_MANIFEST_DIR")).join("../skills").join(name).join("SKILL.md")`.
2. Target: `self.home.path().join(".claude/loom-skill-catalog").join(name).join("SKILL.md")` (the catalog directory name is `CATALOG_DIR_NAME` in `loom/src/skills/index_catalog.rs`; the fixture already writes catalogued skills there in `write_fixture_skills`).
3. `fs::create_dir_all` the target parent, `fs::copy` source to target, then `loom::commands::skill_index::execute_in_agent_dir(&self.home.path().join(".claude"), false)`. Call `.expect("...")` with a short message on each of the three results.

`execute_in_agent_dir` rebuilds `<root>/hooks/loom/skill-keywords.json` from `<root>/skills` and `<root>/loom-skill-catalog`. The fixture skills `write_skill_md` writes carry no `triggers:` and their descriptions carry no `Triggers:` marker, so the rebuilt index holds only the copied skill's keywords. The function stays under 20 lines; `fs` and `Path` are already imported.

## Step 2: the test in `hooks_skill_trigger_evidence.rs`

Add after `phrase_trigger_still_qualifies_a_now_stopword_word`, in the same shape (it opens with an early return on `skip_unless_python3(...)`; the module already has `use super::*;`):

```rust
#[test]
fn go_generic_words_do_not_qualify_golang_on_a_rust_prompt()
```

1. `if skip_unless_python3("hooks_skill_trigger::go_generic_words_do_not_qualify_golang_on_a_rust_prompt") { return; }`, then `let home = FakeHome::new(); home.index_real_skill("loom-golang"); let (_hook_dir, hook) = install_hook();`
2. `let out = run_hook(&hook, &home, "refactor the struct and the map in this rust module", None, false);` assert `out.code == 0` (message carries `out.stderr`) and that `out.stdout` does not contain `loom-golang` (message carries `out.stdout`).
3. `let out = run_hook(&hook, &home, "iterate over a go map inside a goroutine", None, false);` assert `out.code == 0` and that `additional_context(&out.stdout)` contains `loom-golang`. This positive control fails the test if the index rebuild produced nothing.

A short comment above step 2 says why: bare `struct` and `map` used to score 2 together and qualify the Go skill on a Rust prompt; the triggers are now Go-qualified phrases.

Keep the test under 50 lines and each file under 360 lines (both are well under today: 270 and 172).

## Done means

Both edits made and no other file touched. Run no build, no test and no formatter: the integration test target builds the whole crate, which can outlast the 540-second forward deadline. The orchestrator then runs `cargo test --test integration go_generic_words_do_not_qualify_golang_on_a_rust_prompt` and expects it to FAIL at the step 2 assertion, because the trigger list still holds bare `struct` and `map`; after the Claude worker's trigger change the same command passes.

## Report

Files changed, assumptions, and anything you could not resolve.
