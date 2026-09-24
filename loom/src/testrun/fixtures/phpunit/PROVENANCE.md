Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

Exit codes used: `one-pass` 0; `one-fail` 1; `suite` 1; `no-match` 1.
The `no-match` exit code is uncertain across PHPUnit versions; this models PHPUnit 10+ with `--filter` printing `No tests executed!` and exiting 1.
