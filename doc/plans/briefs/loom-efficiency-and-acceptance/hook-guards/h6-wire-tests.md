# H6 — wire the new hook tests into the runner

Tier: haiku (`loom-software-engineer` with `model: haiku`). Spawned after H3, H4 and H5 return.

## Files you own (write)

- `loom-hooks/tests/run-all.sh`

## Steps

1. The main agent's prompt lists the `run_test` lines H3, H4 and H5 reported, plus
   `loom-hooks/tests/loom-control-complete-knowledge.sh`, which exists today with no entry.
2. Add each line beside the existing entries for the same hook, matching their form exactly
   (`rg -n 'run_test' loom-hooks/tests/run-all.sh | head -20`).
3. Confirm every script named exists: `test -f` each path. Report any that does not.

Do not run the suite; the main agent does.
