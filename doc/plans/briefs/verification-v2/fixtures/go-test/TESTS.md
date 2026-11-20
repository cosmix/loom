# go-test fixture

Source project: scratch `runner-projects/go-test/` (module `fixturego`).

`add_test.go`:

- `TestAlphaPasses` (passes)
- `TestBetaFails` (fails: `t.Fatalf` on `Add(2, 2) != 5`)
- `TestGammaPasses` (passes)

Filter command form: `go test ./... -run '^<Name>$' -v`, with `GOCACHE` and
`GOPATH` pointed at `.gocache`/`.gopath` inside the scratch project.

No `.nocolor` variants captured: `go test` output carries no ANSI color
codes by default (no `--no-color`/`NO_COLOR` support to test against).

Notable: `no-match` (filter `^TestDeltaMissing$`) exits **0** — `go test`
treats "no tests matched the -run pattern" as success (prints `testing: warning: no tests to run`).
