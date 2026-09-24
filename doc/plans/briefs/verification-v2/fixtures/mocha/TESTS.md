# mocha fixture

Source project: scratch `runner-projects/mocha/fixture.test.js`, project-local
`mocha@12.0.2` (installed via `bun add -d mocha`), run through `bunx`.
Tests are top-level `it()` blocks (no `describe()` wrapper) so the full
test title is exactly the bare name.

- `alpha_passes` (passes)
- `beta_fails` (fails: `assert.strictEqual(add(2, 2), 5, 'deliberate failure')`)
- `gamma_passes` (passes)

Filter command form: `bunx mocha <file> --grep '<name>'`.

`.nocolor` variants add `--no-color`; output bytes were identical to the
default run (mocha already disables color on the non-TTY pipe used to
capture these files).

Notable: `no-match` (`--grep 'delta_missing'`) exits **0** — mocha reports
"0 passing" and does not fail when `--grep` matches none of the tests.
