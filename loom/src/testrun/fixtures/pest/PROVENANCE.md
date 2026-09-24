Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

Exit codes used: `one-pass` 0, `one-fail` 1, `suite` 1, `no-match` 0.
The `no-match` exit code is uncertain: Pest versions may exit 0 or 1 when a filter finds no tests. The other exit codes follow the documented success/failure convention.
