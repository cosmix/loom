# jest fixture

Source project: scratch `runner-projects/jest/fixture.test.js`, project-local
`jest@30.5.2` (installed via `bun add -d jest`), run through `bunx`.

- `alpha_passes` (passes)
- `beta_fails` (fails: `expect(add(2, 2)).toBe(5)`)
- `gamma_passes` (passes)

Filter command form: `bunx jest <file> -t '<name>'`. Jest writes ALL of its
run output (summary, failures) to stderr, not stdout — stdout is empty in
every scenario here.

`.nocolor` variants add `CI=1 --colors=false` (plain `-t` runs already
carried no ANSI codes on the redirected pipe, but `--colors=false` is the
documented flag and was included for a stable, explicit no-color form).

Notable: `no-match` (`-t 'delta_missing'`) exits **0** — jest reports
"Tests: 3 skipped, 3 total" / "Test Suites: 1 skipped, 0 of 1 total" and
does not fail when a `-t` pattern matches none of a file's tests.
