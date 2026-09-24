# unittest fixture

Source project: scratch `runner-projects/unittest/test_fixture.py`.

`class FixtureTests(unittest.TestCase)`:

- `test_alpha_passes` (passes)
- `test_beta_fails` (fails: `self.assertEqual(add(2, 2), 5, "deliberate failure")`)
- `test_gamma_passes` (passes)

(unittest's default loader only discovers methods with a `test` prefix, so
the idiom is `test_alpha_passes` etc. rather than bare `alpha_passes`.)

Filter command form: `python3 -m unittest <module>.<Class>.<method> -v`.
unittest writes ALL of its output (dots/verbose lines, tracebacks, summary)
to stderr, not stdout — stdout is empty in every scenario here.

No `.nocolor` variants captured: unittest's default runner has no ANSI
color output and no `--no-color`/`NO_COLOR` support.

Notable: `no-match` (`test_fixture.FixtureTests.test_delta_missing`, which
does not exist) exits **1** — unittest fails to load the named test
(`AttributeError` wrapped as a load error) and reports it as an error in
the run, not a silent zero-match success.
