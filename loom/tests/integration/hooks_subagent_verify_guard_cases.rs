//! Case tables for `hooks_subagent_verify_guard.rs`.
//!
//! Split out purely for size: the harness (process-tree construction, hook
//! installation, env scrubbing) and the data it is driven by grow for
//! different reasons, and together they exceeded the 400-line file cap.
//!
//! Every case is hand-verified against the hook's CORRECT (intended)
//! behaviour, not against what it happened to do when the case was written.
//! `REGRESSION:` cases pin tokenizer edge cases that once misclassified a
//! command - redirections read as test filters, embedded newlines stranding
//! every line after the first, subshells and brace groups hiding a runner,
//! quoted separators starting a fake command, and shell metacharacters
//! written without surrounding spaces. Do not weaken one to make a run pass.

pub const BLOCK_CASES: &[(&str, &str)] = &[
    ("cargo test", "bare cargo test"),
    (
        "cargo test --manifest-path loom/Cargo.toml",
        "manifest-path alone is not a scope",
    ),
    ("cargo test --all", "explicit --all"),
    ("cargo build", "cargo build is project-wide in any form"),
    ("cargo clippy", "cargo clippy is project-wide in any form"),
    ("cargo fmt", "cargo fmt is project-wide in any form"),
    (
        "cd loom && cargo clippy --all-targets",
        "clippy after a cd prefix",
    ),
    (
        "env RUST_LOG=debug cargo test",
        "env-prefixed bare cargo test",
    ),
    ("pytest", "bare pytest"),
    ("go test ./...", "go test walks the whole module"),
    ("make test", "make test"),
    ("npm run build", "npm run build with no path"),
    // --- Regressions: tokenizer edge cases, expected to still fail
    // until the concurrent hook fix lands (do not weaken - re-run) ---
    (
        "cargo test 2>&1 | tail -50",
        "REGRESSION: stdout/stderr redirection must not count as a scope",
    ),
    (
        "cargo test > out.log",
        "REGRESSION: output redirection must not count as a scope",
    ),
    (
        "cd loom\ncargo test",
        "REGRESSION: newline-separated command must still be inspected",
    ),
    (
        "echo \"x\"\ncargo clippy",
        "REGRESSION: newline after a quoted echo must not hide cargo clippy",
    ),
    (
        "cargo nextest run",
        "REGRESSION: cargo nextest run is project-wide like cargo test",
    ),
    (
        "cargo test --doc",
        "REGRESSION: --doc runs every doc-test in the crate, not scoped",
    ),
    (
        "(cd loom && cargo test)",
        "REGRESSION: a subshell must not hide cargo test",
    ),
    (
        "go build ./...",
        "REGRESSION: go build walks the whole module",
    ),
    ("go vet ./...", "REGRESSION: go vet walks the whole module"),
    // --- Regressions: shell metacharacters written WITHOUT surrounding
    // spaces. IFS splitting breaks on whitespace only, so a glued `;`,
    // `&&` or `|` never became its own token: the separator was invisible,
    // the runner sat at a non-command position, and `cargo test;` hid the
    // subcommand behind `test;`. Every form below is ordinary shell an
    // agent writes without thinking, so this class must stay pinned. ---
    (
        "echo hi; cargo test",
        "REGRESSION: `;` glued to the previous word still separates commands",
    ),
    (
        "cargo test; echo done",
        "REGRESSION: `test;` is still the test subcommand",
    ),
    (
        "echo hi ;cargo test",
        "REGRESSION: `;` glued to the FOLLOWING word still separates commands",
    ),
    (
        "echo hi&&cargo test",
        "REGRESSION: `&&` with no surrounding spaces still separates commands",
    ),
    (
        "cd loom&&cargo build",
        "REGRESSION: the natural `cd x&&cargo build` form must not slip through",
    ),
    (
        "true||cargo build",
        "REGRESSION: `||` with no surrounding spaces still separates commands",
    ),
    (
        "echo hi|cargo test",
        "REGRESSION: `|` with no surrounding spaces still separates commands",
    ),
    (
        "{ cargo test; }",
        "REGRESSION: a brace group must not hide cargo test",
    ),
    (
        "if true; then cargo test; fi",
        "REGRESSION: an if-block body must still be inspected",
    ),
    (
        "while true; do cargo test; done",
        "REGRESSION: a while-block body must still be inspected",
    ),
    (
        "for f in a; do cargo build; done",
        "REGRESSION: a for-block body must still be inspected",
    ),
];

pub const ALLOW_CASES: &[(&str, &str)] = &[
    ("cargo test signals::", "scoped filter"),
    (
        "cargo test --manifest-path loom/Cargo.toml --test integration hooks_",
        "scoped --test with a filter",
    ),
    ("cargo test -p loom", "scoped package"),
    (
        r#"rg "cargo test" doc/"#,
        "a quoted mention inside an rg pattern is not a command",
    ),
    (
        "echo make test",
        "make/test as echo arguments, not a real make invocation",
    ),
    ("pytest tests/test_foo.py", "scoped pytest path"),
    ("go test ./pkg/...", "scoped go package"),
    (
        r#"git commit -m "add cargo test for X""#,
        "git commit is not a verification runner",
    ),
    ("ls -la", "unrelated command"),
    // --- Regressions ---
    (
        "make docs\ncargo test --test parser",
        "REGRESSION: a preceding unrelated make target must not blame make \
         for the scoped cargo test that follows on the next line",
    ),
    (
        r#"rg "x && cargo build -v" src/"#,
        "REGRESSION: a quoted && inside an rg pattern must not reset command position",
    ),
    (
        r#"rg "a;b" src/"#,
        "REGRESSION: padding a glued `;` must not make a quoted one a separator",
    ),
    (
        r#"rg "cargo|npm" src/"#,
        "REGRESSION: an alternation regex is the commonest quoted `|`; padding \
         `|` must leave the word after it inert rather than in command position",
    ),
    (
        r#"jq -r ".a|.b" file.json"#,
        "REGRESSION: a quoted `|` in a jq filter is not a pipe",
    ),
    (
        r#"echo "a;cargo test""#,
        "REGRESSION: a `;` inside quotes must leave the following word inert",
    ),
    (
        "cargo test knowledge:: 2>&1 | tail -50",
        "REGRESSION: padding `|` must not disturb a scoped run's redirection",
    ),
];
