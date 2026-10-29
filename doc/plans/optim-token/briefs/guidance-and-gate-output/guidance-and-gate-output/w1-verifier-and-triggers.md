# W1: Go skill triggers

Tier: haiku (`loom-software-engineer`). The work is exact text. Working directory for cargo: `loom/`. Paths below are repo-relative.

## Files you own

- `skills/loom-golang/SKILL.md` (the `triggers:` list only)

Read-only: `loom/tests/integration/hooks_skill_trigger_evidence.rs`, `loom/src/commands/skill_index.rs` (`STOPWORDS`).

The codex unit C2 ran before you. It wrote `go_generic_words_do_not_qualify_golang_on_a_rust_prompt` in `loom/tests/integration/hooks_skill_trigger_evidence.rs`, which copies the real `skills/loom-golang/SKILL.md` into a fake catalog and fails until your change lands. Do not edit that file.

## The triggers (`skills/loom-golang/SKILL.md`, lines 4-31)

`_rank` in `loom-hooks/skill-trigger.sh:265-279` qualifies a skill on two single-word hits (score 1 each) or one phrase hit (score 2, `_score_keywords`, `:194-201`). Today `struct` plus `map` qualifies loom-golang on a Rust prompt. Replace the `triggers:` list with exactly this list, in this order, and change nothing else in the file:

```yaml
triggers:
  - golang
  - goroutine
  - in go
  - go code
  - go package
  - go module
  - go channel
  - go interface
  - go struct
  - go pointer
  - go slice
  - go map
  - go defer
  - go context
  - error
  - gin
  - echo framework
  - fiber
  - cobra
  - viper
  - gorm
  - sqlx
  - go mod
  - go test
  - effective go
  - errgroup
  - sync
  - go mutex
  - waitgroup
  - race detector
```

The bare words `go`, `channel`, `interface`, `struct`, `pointer`, `slice`, `map`, `defer`, `mutex` and `echo` are gone. `error` and `sync` stay as they are: both are in `STOPWORDS` (`loom/src/commands/skill_index.rs:34-43`), so the indexer already drops them. `STOPWORDS` itself is untouched: `stopwords_match_between_shell_and_rust_index` (`hooks_skill_trigger_evidence.rs:151-172`) pins the shell and Rust copies equal.

## Your one check

`cargo test --test integration go_generic_words_do_not_qualify_golang_on_a_rust_prompt`, run once, from `loom/`. The main agent runs the gate. If the test fails because its expectations differ from this brief, report the exact assertion; never edit the test file.
