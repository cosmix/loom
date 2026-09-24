Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

Exit codes used:

- `0` for a passing run (one-pass): expected standard success status.
- `2` for runs with ExUnit test failures (one-fail, suite): ExUnit's default failure status; uncertain across versions or custom configuration because this output was not captured.
- `1` for `--only` with no executed test (no-match and no-match.excluded): expected since Elixir 1.12; uncertain across versions because this output was not captured.
- `1` for a compilation error (build-error): expected Mix compile failure status; unconfirmed on the plan author host.

The two no-match forms represent version-dependent summary reporting: `0 tests, 0 failures` and an all-excluded test count. Both mean that no test executed.
