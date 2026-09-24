# language-skills / W3 — Ruby, PHP, Elixir skills

Read first: `doc/plans/briefs/verification-v2/language-skills/SPEC.md` (binding shape and the
adapter section); `doc/plans/briefs/verification-v2/DESIGN.md` D5, D6, D7;
`skills/loom-python/SKILL.md` and `skills/loom-rust/SKILL.md` as shape references.

## Files you own

`skills/loom-ruby/SKILL.md`, `skills/loom-php/SKILL.md`, `skills/loom-elixir/SKILL.md` (all new).

## Scope per skill

- `loom-ruby`: Bundler, Ruby 3 features, blocks/procs, Rails essentials, RSpec and Minitest. Adapters `rspec` and `minitest`.
- `loom-php`: Composer, PHP 8 types, enums and readonly, Laravel/Symfony essentials, PHPUnit and Pest. Adapters `phpunit` and `pest`.
- `loom-elixir`: mix, processes and OTP (GenServer, supervisors), pattern matching, Phoenix and Ecto essentials, ExUnit. Adapter `mix-test`.

Each skill follows SPEC.md's section order, is 350–550 lines, and has the exact heading
`## Loom Test Runner Adapter`. Commands, `test` value formats, detection rules and no-match
behaviour come from DESIGN D5/D7 verbatim. Where D5 marks a runner "documented", say that
loom's parser for it was written from the runner's documented output.

## Proof (one command, once)

`rg -c "^## " skills/loom-ruby/SKILL.md skills/loom-php/SKILL.md skills/loom-elixir/SKILL.md`
(each file shows the eight required headings or more).

## Report

Files created with their line counts; any D5/D7 fact you found wrong for the runner, which the
main agent records in loom memory.
