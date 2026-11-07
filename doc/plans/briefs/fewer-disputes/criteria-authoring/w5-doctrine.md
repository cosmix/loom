# criteria-authoring W5: plan-writer doctrine and README

Tier: haiku. Wave 1, with W0, W0b and W4. Read `common.md` first (sections "Where you are", "The design in six
lines", "Code rules" last bullets). You write prose only; no Rust.

## Files you own

`../skills/loom-plan-writer/SKILL.md`, `../skills/loom-plan-writer/references/v2-contracts.md`,
`../skills/loom-plan-writer/references/verification-rules.md`, `../skills/loom-plan-writer/references/sandbox.md`,
`../README.md`, `../skills/loom-wiring-test/SKILL.md`.

## Do not touch

- `references/v2-contracts.md` Section 5 ("What completion enforces in v2") and `references/bookend-stages.md`:
  the next stage (gate-noise-and-review) owns them.
- `SKILL.md` Section 9's last sentence of its first paragraph ("Filing a dispute ends the stage session by
  design; ..."): the stage after that owns it. You add one sentence of your own to that paragraph, before it.
- Style: no stock words, no "not X but Y" cadence, no invented hyphen compounds, every fenced block names its
  language. Keep each file's existing voice and table shapes. `rg` the file for the text you replace and
  reconcile every other mention (the "Self-consistency sweep" rule).

## Edits

### `SKILL.md`

1. Section 2, "After writing: validate, self-review, STOP", step 1: after "Fix and re-run until it passes."
   insert: `Then run loom plan verify --strict --dry-run doc/plans/PLAN-<name>.md from a shell where loom can
   confine commands (a stage session cannot nest bubblewrap): it runs the plan's
   loom.provision entries in a checkout of HEAD, then every acceptance, after_stage and wiring_tests command
   once, and names environment failures and timeouts as errors before any stage runs. An empty test
   selection, and a criterion that fails there only because its stage has not built anything yet, is a note
   and expected; one that errors is a planning defect to fix.` (Wrap the command in
   backticks.) Add the same one-clause reminder to the "Pre-STOP checklist" bullet that ends "`loom plan
   verify --strict` passes": `...; loom plan verify --strict --dry-run reports no error.`
2. Section 6 table: the `contracts` (v2) row says `test` is an optional hint the contract writer replaces;
   add a row `consumer_deferred` | A stage's new units have no consumer anywhere in the plan | `consumer_deferred:
   "<reason>"`: one line, at most 300 bytes. Add a `reachable` mention to the wiring-coverage sentence of the
   leaf rule if one exists. Ground the wording before writing it: a `version: 2` plan never meets the leaf rule
   ("Standard leaf stage has artifacts but no wiring"), because every version 2 standard stage is an ancestor
   of integration-verify; what a version 2 plan under `--strict` meets is the cross-stage coverage warning
   (`artifacts are not covered by wiring checks`, `check_cross_stage_wiring_coverage`). Say that
   `consumer_deferred` and a non-empty `reachable` list satisfy that warning, and that `consumer_deferred` also
   satisfies the leaf rule of a `version: 1` plan.
3. Section 6, the paragraph that starts `**⛔ wiring MUST target the CONSUMER, not the PRODUCER.**`: keep its
   first two sentences and replace the rest with:
   `Prove entry-point reach with reachable. Write wiring only for a seam the source graph cannot see:
   registration by string, configuration, templates, macros, trait objects. Never point a wiring pattern at a
   definition (fn, mod, struct, enum, trait, impl, type, def, class, function or interface, with or without a
   visibility prefix): loom plan verify rejects it in a version 2 plan and warns in version 1, and completion
   fails a pattern that matches only the definition of a name its file defines. When the consumer of a stage's
   new units arrives in a later plan, declare consumer_deferred: "<reason>" on the stage instead of inventing
   a check: it satisfies the wiring-coverage warning of a version 2 plan and the leaf rule of a version 1 plan, appears
   in the stage's signal, and waives nothing else. A stage with a non-empty reachable list satisfies the
   coverage warning too. Pair every wiring entry with a behavioural
   acceptance command or wiring_tests entry where one exists.` (Backtick the field names as the file does.)
4. Section 6, "Timeouts" rule: replace the sentence with `Timeouts: each wiring_tests command has a 30 s cap,
   an after_stage or before_stage check defaults to 30 s, and an acceptance command defaults to 300 s whether it
   is a string or the extended form; the extended form can set a timeout up to 3600 s. See
   references/sandbox.md#where-criteria-run. Prefer focused commands.` (Keep the markdown link style of the
   original.)
5. Section 9, first paragraph: before "Filing a dispute ends the stage session by design", insert `Run loom
   plan verify --strict --dry-run before handing a plan over: it executes every criterion against HEAD under
   the confinement completion uses, so a command that needs a socket, the network or a tool the confined
   runner lacks, or a criterion that times out, shows up as an error before a stage spends a dispute on it; a
   test filter that selects nothing at base is a note, since the tests it names may be the stage's own.`
   Leave the sentence that follows untouched.

### `references/v2-contracts.md` (Sections 1-4 only)

1. Section 1, step 2: after the adapter sentence add `Name a contract's test only when you can give the exact
   identifier; otherwise omit it and the contract writer names it with loom stage contracts freeze --test
   <contract-id>=<identifier>. The stage description must still state the public surface (Section 3).`
2. Section 3 table: `test` becomes `optional hint; the runner-specific identifier in the form the language
   skill gives (PHPUnit and Pest require a fully qualified Class::method, Swift XCTest requires
   Module.Class/method). The contract writer's --test replaces it`. `cwd` and `runner` rows gain: `Leave
   unset when the right value depends on the code: the contract writer may pass --cwd <id>=<dir> (an
   ancestor of the file, inside working_dir, holding the runner's root marker as committed at the stage's
   base) or --runner <id>=<adapter> (a known adapter). A value you set wins and the writer cannot override
   it.`
3. Section 3, the sequence list, step 2: say the writer runs `loom stage contracts freeze <stage-id> --test
   <id>=<identifier> [--runner <id>=<adapter>] [--cwd <id>=<dir>]` (each flag repeats; a later freeze keeps
   the earlier flags it does not repeat), and that the freeze also refuses when a pure check of the stage
   fails on a file the writer wrote (a contract file, a harness file or a file the freeze adds): a formatter
   check, a lint (`eslint`, `biome lint`, `biome check`, `ruff check`, and `cargo clippy`, `go vet`,
   `golangci-lint run` once every contract compiles; `tsc --noEmit` never runs at freeze) or a simple
   `rg`/`grep` command, reported with the failing output the way a format refusal is. A build failure stays
   an accepted red outcome. The freeze waits for the host run and for the daemon's application of it, like loom check: a refusal that
   names a file the writer wrote comes back as Host job: failed with the failing output and a line that says
   whether it was returned (n of 3) or filed, the writer fixes the file and freezes again. Use these words: a
   static refusal is returned to the writer; it is filed as a freeze-refusal dispute on the third static refusal
   in the session, or when the writer stays quiet 5 minutes after a refusal. Step 3: the record keeps each contract's `test`, `adapter` and `cwd`, and
   completion, the signal's frozen-contracts table, `loom stage contracts show` and the adjudication prompts
   read the record.
4. Section 4, "`wiring` in v2": add two bullets after "Definition-site exclusion": `Definition-shaped
   patterns.` loom plan verify rejects (an error in a version 2 plan, a warning in version 1) a pattern whose
   text, after leading whitespace and a visibility prefix (pub, pub(crate), pub(super), export, export
   default, async), starts with fn, mod, struct, enum, trait, impl, type, def, class, function or interface.
   `consumer_deferred.` A stage whose new units have no consumer in the plan declares `consumer_deferred:
   "<reason>"` (one line, at most 300 bytes, no control characters) instead of a wiring check. It satisfies
   the cross-stage coverage warning that pushes a version 2 plan under `--strict` to invent a wiring check, and
   the leaf rule ("Standard leaf stage has artifacts but no wiring", which only version 1 plans meet), reaches
   the stage signal and integration-verify's reviewers, and waives nothing else: reachable, the unwired-file detection and the
   definition-site rule still apply. Also state in the `reachable` subsection that a non-empty `reachable` list
   satisfies the coverage warning.

### `references/verification-rules.md`

In "Every gate must be GREEN at BASELINE", item 2 ("Run it where the STAGE will run it"), append: `loom plan
verify --strict --dry-run runs every acceptance, after_stage and wiring_tests command in one checkout of the
committed HEAD (never the working tree: commit what a command reads) under the stage's confinement and timeout,
and reports environment failures (a bubblewrap or namespace error, a socket or network denial, and exit 126 or
127 for a stage with no dependencies), timeouts and empty test selections as errors. Before the criteria it runs
the plan's loom.provision entries in that checkout, so provisioned tools exist. A criterion that already passes
is a note, and a warning only when every acceptance criterion of a stage with no dependencies passes (the
stage could not tell done from not done); a plain failure is a note, and so is exit 126 or 127 in a stage with
dependencies, where an earlier stage may provide the program or script. `--strict --dry-run` therefore passes
on a plan with sound criteria. A stage with dependencies is expected to fail at base; only its environment
classes mean anything there. A command that times
out runs once more in the same checkout, whose build cache the first run warmed; only a second timeout is
reported, so a cold build alone is not an error, but a package that still exceeds its criterion's timeout with a
warm cache needs a larger timeout_secs on that criterion.`

### `references/sandbox.md`

In "Where criteria run", replace the sentence `A string criterion gets 300 s. The extended form ... defaults to
30 s and takes timeout_secs from 1 to 3600: set it for a slow suite.` with `A string criterion gets 300 s. The
extended form {command: ..., timeout_secs: N} defaults to 300 s too and takes timeout_secs from 1 to 3600: set
it for a slow suite. An after_stage or before_stage check defaults to 30 s and a wiring_tests command is capped
at 30 s.` After the paragraph that ends with `--criterion <n>` re-runs, add: `loom plan verify --dry-run runs
the same commands against a checkout of HEAD before any stage exists, after running the plan's provision
entries there; it needs a shell where bubblewrap can create namespaces.`

### `../README.md`

- Line with `loom plan verify <plan-path> [--strict] [--json] [--no-color]` gains `[--dry-run]`.
- The `loom plan verify` paragraph (starts `` `loom plan verify` validates a plan file without touching ``): add
  two sentences before `Exits 0 on success`: `--dry-run additionally runs every acceptance, after_stage and
  wiring_tests command once against a checkout of HEAD under each stage's confinement and timeout, and
  reports environment failures, timeouts and empty test selections as errors, a criterion that already passes
  as a note (a warning only when every acceptance criterion of a stage with no dependencies passes), and a
  plain failure as a note; it first runs the plan's `provision` entries in that checkout and needs a shell
  where loom can confine commands. A wiring pattern that starts with a definition keyword is an error in a version 2 plan and
  a warning in version 1.`
- The `loom stage contracts freeze|show|restore` usage line: `freeze` gains `[--test <id>=<identifier>]...
  [--runner <id>=<adapter>]... [--cwd <id>=<dir>]...`.
- The version 2 paragraph that says `ends with loom stage contracts freeze`: add that the writer names each
  contract's test (and, where the plan leaves them open, runner and directory) on that command, and that the
  freeze also runs the stage's lint and search checks over what the writer wrote.
- The stage fields table: add a `consumer_deferred` row (`No`) after `skills`: `One line (at most 300 bytes) saying
  why no consumer of the stage's new units exists in the plan; satisfies the wiring-coverage warning
  and the version 1 leaf rule for a stage with artifacts and no wiring, and waives nothing else`.
- `rg -n '30 s|30 seconds|Extended 30' ../README.md` and fix any statement that an extended acceptance
  criterion defaults to 30 s.

### `../skills/loom-wiring-test/SKILL.md` (stale text no other worker owns; line numbers advisory, `rg` for the text)

The skill still teaches wiring for entry-point reach (against Decision 7) and states Extended criteria at 30 s.
Make exactly these edits and leave the rest:

1. Line ~36, the `acceptance` row of the table: `5 min (Simple) / 30 s (Extended)` becomes `5 min (Simple and
   Extended; Extended takes timeout_secs up to 3600)`. Line ~109, the comment in the example:
   `# Extended (TruthCheck): 30-s timeout` becomes `# Extended (TruthCheck): 5-min default, timeout_secs up
   to 3600`. Add to the table row of `wiring_tests` nothing; the `wiring_tests` cap stays 30 s and an
   `after_stage` check defaults to 30 s (say so in one clause after the table: `after_stage and before_stage
   checks default to 30 s and a wiring_tests command is capped at 30 s`).
2. Section "The one rule that matters: verify the CONSUMER, not the PRODUCER" (lines ~50-60): after its first
   paragraph insert: `In a version 2 plan, prove entry-point reach with a reachable check (symbol and from,
   see the loom-plan-writer skill, references/v2-contracts.md Section 4): it walks the source graph from the
   entry point, which a regex on one file cannot. Write a wiring entry only for a seam the graph cannot see:
   registration by string, configuration, templates, macros, trait objects. Never start a pattern with a
   definition keyword (fn, mod, struct, enum, trait, impl, type, def, class, function or interface, with or
   without pub, pub(crate), pub(super), export, export default or async): loom plan verify rejects it in a
   version 2 plan and warns in version 1. When the consumer arrives in a later plan, declare
   consumer_deferred: "<reason>" on the stage instead of inventing a check.`
3. The table under it: keep the three rows (they show producer patterns that fail) and change the consumer cell
   of rows 1 and 3 so they no longer teach a wiring regex for reach: row 1 becomes `reachable: [{symbol:
   NewCommand, from: main, description: ...}]` (dispatch reach), row 3 becomes `reachable: [{symbol: Foo, from:
   App, description: ...}]` (render reach); row 2 (route registration by string) stays a wiring regex.
4. The "CLI command" template (line ~126-129): replace the `wiring:` block (`src/cli/dispatch.rs`,
   `Commands::NewCmd`) with a `reachable:` block (`symbol: run_new_cmd`, `from: main`, `description: "Command
   reachable from the CLI entry point"`) and keep its comment about the consumer. Leave every other template.
5. Checklist line ~212 (`Every wiring targets a CONSUMER site ...`): append `; entry-point reach is a
   reachable check in a version 2 plan, wiring is for seams the graph cannot see`.
Then `rg -n '30 s|30-s|Extended' ../skills/loom-wiring-test/SKILL.md` and fix any remaining statement that an
Extended acceptance criterion defaults to 30 s.

## Done

Run no check (the crate does not compile until wave 2). Before editing `SKILL.md`, read
`src/orchestrator/signals/tests_doctrine.rs` and `tests_doctrine_v2.rs` for every string they look for in
`PLAN_WRITER_SKILL` (they include `../loom-orchestration/SKILL.md#rule-7--model-allocation`, `loom project detect`
and `references/v2-contracts.md`) and keep each pinned phrase verbatim in the file.
