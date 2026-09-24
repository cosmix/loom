# minitest fixture

Source project: scratch `runner-projects/minitest/fixture_test.rb`, plain
Ruby stdlib `minitest/autorun` (gem `minitest 5.20.0` bundled with Ruby 3.3.8).

`class FixtureTest < Minitest::Test`:

- `test_alpha_passes` (passes)
- `test_beta_fails` (fails: `assert_equal(5, add(2, 2), 'deliberate failure')`)
- `test_gamma_passes` (passes)

(minitest's default runner only picks up methods with a `test_` prefix, so
the idiom is `test_alpha_passes` etc. rather than bare `alpha_passes`.)

Filter command form: `ruby <file> -n '/^<name>$/'`.

No `.nocolor` variants captured: minitest's default Progress reporter has
no ANSI color output on a non-TTY pipe and no `--no-color`/`NO_COLOR` flag.

Notable: `no-match` (`-n '/^test_delta_missing$/'`) exits **0** —
"0 runs, 0 assertions, 0 failures, 0 errors, 0 skips" is treated as a
successful (empty) run.
