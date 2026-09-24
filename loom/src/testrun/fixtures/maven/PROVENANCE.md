Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

Exit codes used:

- `0`: one-pass.
- `1`: one-fail, suite, no-match (Surefire's default `-Dtest` no-match failure), build-error (compilation failure).

Uncertain exit codes: none for these documented default behaviors. Output wording and placement should be checked against a captured Maven run.
