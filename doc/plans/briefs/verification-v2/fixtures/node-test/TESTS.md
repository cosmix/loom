# node-test fixture

Source project: scratch `runner-projects/node-test/fixture.test.js` (CommonJS, `node:test`).

- `alpha_passes` (passes)
- `beta_fails` (fails: `assert.strictEqual(add(2, 2), 5, 'deliberate failure')`)
- `gamma_passes` (passes)

Filter command form: `node --test --test-name-pattern='^<name>$' <file>`.

`.nocolor` variants use `NO_COLOR=1`. Node's test runner already detects
the non-TTY redirect used to capture these files and prints TAP-style
output without ANSI codes by default, so the default and `.nocolor` bytes
are effectively identical here — captured anyway for completeness.

Notable: `no-match` (pattern `^delta_missing$`, matching none of the three
tests) exits **0** — the run reports 0 passing / 0 failing (all three
existing tests are skipped for not matching the pattern) and that is not
treated as a failure.
