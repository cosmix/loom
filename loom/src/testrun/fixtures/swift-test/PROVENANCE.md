Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

Exit codes used: `one-pass` 0; `one-fail` 1; `no-match` 0; `suite` 1; `build-error` 1.

The `no-match` exit code is uncertain across Swift toolchains. Its fixture uses exit 0 and an `Executed 0 tests` summary; the parser relies on that summary to classify it.
