# Task: make loom's signals prevent contracts that run in the wrong environment

## Incident (loom 1.2.0, plan product-blog-kit)

- The repo has no root Python project. Each package (`post`, `tests`, `fixtures/web`,
  `fixtures/tui`) has its own `pyproject.toml` with `[tool.pytest.ini_options]` and
  `uv.lock`. Every contract set `runner: pytest` and omitted `cwd`.
- Loom recorded `cwd=.` for all 14 contracts, against its documented fallback (the
  nearest ancestor holding runner config). From the root, the adapter ran
  `python3 -m pytest <file>::<test>` on system python3, which has no pytest.
- Freeze: every contract exited 1 with `No module named pytest`. Loom froze them with
  outcome `exit_nonzero_unverified`. Nothing in the contract session's signal told it
  that this outcome means the red is unproven.
- Implementation: the agents ran the tests their own way (`cd <pkg> && uv run pytest`),
  saw green, passed the gate and review, and committed.
- Completion: `loom stage complete` ran the contracts from the root again and failed
  them all. Disputes fix one contract per filing with a budget of 3, so neither stage
  could repair 7 contracts. Both stages blocked with finished, correct work.
- The adjudicator accepted the cwd fix, but its verdict said it "could not confirm the
  pytest adapter picks up the uv environment from that cwd". It had no way to run a
  contract the way loom runs it.
- Operator repair: `loom stage amend --field contracts` set `cwd` on each contract and
  both stages completed. The amend changed the plan and stage file; the freeze record
  (`loom stage contracts show`) still reads `cwd=.`. Completion turned out to read the
  stage file, so the record shown to agents and operators is stale after an amend.

## The class to cover

The command loom runs for a contract differs from the command the agents used while
writing, implementing, and judging it, and nothing shows the difference before
completion. Members include:

- wrong cwd
- a different adapter detected (pytest vs unittest)
- `uv run` vs bare `python3`, depending on whether a lockfile sits in the cwd
- runner or dependencies missing in that environment
- the file path resolving differently from the cwd
- the test not selected (exit 4/5, "no tests ran")
- harness/conftest not loaded
- sandbox network or port limits at completion
- reds caused by the environment and frozen as if they were real

Fix the class, not only cwd.

## Required changes

1. **One reproducible command.** Add a read-only way to run the contracts exactly as
   completion will, e.g. `loom stage contracts run <stage> [--id <contract>]`. Print,
   per contract, the resolved cwd, the adapter, the full command, the exit code, and
   the classified outcome. Use the same code path as freeze and completion. Every
   signal below refers to it, so agents never have to guess loom's invocation.
2. **Contract session signal** (writes and freezes contracts):
   - Set `cwd` on every contract, naming the directory that holds the test's package
     manifest and lockfile.
   - Before freezing, run the command from change 1 and read each resolved command.
   - A red counts only when the test ran and failed for the reason in `rejects`.
     Environment reds are invalid and must be fixed before freezing: runner or module
     missing, `command not found`, a missing dependency, "no tests ran", a collection
     error from anything other than a not-yet-written name.
   - After freezing, read `loom stage contracts show`. Treat `exit_nonzero_unverified`,
     or a cwd that is not the test's package, as a failed freeze to fix in the same
     session.
3. **Implementation stage signal**:
   - Before the gate and before committing, run the contracts with the command from
     change 1, not with the agent's own test command, and report its output.
   - If a contract fails only there, that is a contract-metadata defect. Say so, and
     name every affected contract in one dispute or block, not one per filing.
4. **Adjudicator signal**: reproduce with the command from change 1 before ruling, and
   cite its output. A metadata fix (cwd/runner) that applies to several contracts of
   the stage is applied to all of them in one verdict.
5. **Operator guidance in block messages and `loom status`**: when a block concerns
   contract execution, print the resolved command per contract and the
   `loom stage amend --field contracts` repair. This shortens recovery for the next
   operator.
6. **Plan-writer skill and `loom plan verify`**: in a repository with more than one
   package (more than one manifest or lockfile), require `cwd` on every contract and
   reject a cwd that is not an ancestor of the file. Warn when a contract's file lies
   under a package that `loom project detect` does not list.

## Mechanical checks (the signals are advice; these enforce it)

- **Freeze:** refuse a red whose output shows the runner or its module is missing, and
  refuse `exit_nonzero_unverified` unless the session overrides it explicitly. A wrong
  red then cannot be frozen.
- **cwd resolution:** fix the fallback so an omitted cwd resolves to the nearest
  ancestor with runner config even when `runner:` is set, and record which rule chose
  it.
- **Amend:** a contracts amend updates the freeze record's cwd/adapter too (or `show`
  reads them from the stage file), so `show` matches what completion runs.
- **Disputes:** let one contract dispute cover several contracts that share a stated
  root cause, counted as one filing.

## Acceptance

- A fixture repo with two uv packages and no root project, contracts without cwd:
  plan verify flags them. If they are forced through, freeze refuses the
  `No module named pytest` reds.
- The same fixture with correct cwd freezes with real reds. After implementation,
  `contracts run`, completion, and `contracts show` all report the same cwd and command.
- An amend of a contract's cwd changes what `contracts show` prints.
- Snapshot tests of the three signals contain the new instructions. Each signal names
  the `contracts run` command.
