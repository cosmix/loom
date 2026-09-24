Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

Exit codes used (none uncertain):

- `0`: tests passed (`one-pass`).
- `4`: no tests selected, with nextest's default no-tests action (`no-match`).
- `100`: one or more tests failed (`one-fail`, `suite`).
- `101`: test compilation failed (`build-error`).

These are the documented `NextestExitCode` values. Output is illustrative, not a capture.
