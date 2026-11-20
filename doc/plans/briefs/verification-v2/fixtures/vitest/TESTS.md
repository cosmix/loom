# vitest fixture

Source project: scratch `runner-projects/vitest/fixture.test.js`, project-local
`vitest@5.0.1` (installed via `bun add -d vitest`), run through `bunx`.

- `alpha_passes` (passes)
- `beta_fails` (fails: `expect(add(2, 2)).toBe(5)`)
- `gamma_passes` (passes)

Filter command form: `bunx vitest run <file> -t '<name>'`.

`.nocolor` variants use `CI=1 NO_COLOR=1`; output bytes were identical to
the default run (vitest already disables color on the non-TTY pipe used to
capture these files, same as bun/node above).

Notable: `no-match` (`-t 'delta_missing'`) exits **0** — all 3 tests are
filtered out ("1 skipped" file, "3 skipped" tests) and the run completes
with 0 executed, exit 0.
