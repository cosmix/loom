# bun-test fixture

Source project: scratch `runner-projects/bun-test/fixture.test.js` (`bun:test`).

- `alpha_passes` (passes)
- `beta_fails` (fails: `expect(add(2, 2)).toBe(5)`)
- `gamma_passes` (passes)

Filter command form: `bun test <file> -t '<name>'`.

`.nocolor` variants use `NO_COLOR=1`; bytes were identical to the default
run in every scenario (bun already suppresses color on the non-TTY pipe
used to capture these files).

Notable: `no-match` (`-t 'delta_missing'`) exits **1**, unlike cargo/go/node
which exit 0 on zero matches — bun prints
`error: regex "delta_missing" matched 0 tests. Searched 1 file (skipping 3 tests)`
to stderr and fails the run.
