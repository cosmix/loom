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
    // --- Regressions: tokenizer edge cases (do not weaken) ---
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
    // --- Every test runner the adapters know: an unfiltered run is blocked ---
    ("vitest run", "bare vitest"),
    ("bunx vitest run", "vitest behind bunx"),
    ("npx jest", "bare jest behind npx"),
    ("pnpm exec vitest run", "vitest behind pnpm exec"),
    ("mocha", "bare mocha"),
    ("rspec", "bare rspec"),
    ("vendor/bin/phpunit", "bare phpunit"),
    ("dotnet test", "bare dotnet test"),
    ("mvn clean test", "mvn test with no -Dtest"),
    ("./gradlew test", "gradle test with no --tests"),
    ("ctest --output-on-failure", "ctest with no -R"),
    ("swift test", "bare swift test"),
    ("dart test", "bare dart test"),
    ("flutter test", "bare flutter test"),
    ("mix test", "bare mix test"),
    ("node --test", "bare node --test"),
    ("uv run pytest", "pytest behind uv run"),
    ("python -m pytest", "pytest as a module"),
    ("go test -v ./...", "go test of every package"),
    // --- Every remaining adapter runner (loom/src/testrun/adapters): the
    // guard must classify a command as the adapter's `is_full_run` does ---
    ("sbt test", "bare sbt test task"),
    ("sbt clean test", "sbt test task behind another task"),
    (
        "sbt app/test",
        "sbt project-scoped test task is still the whole suite",
    ),
    ("python -m unittest", "unittest as a module"),
    (
        "python3 -m unittest discover",
        "unittest discover with no selector",
    ),
    ("uv run python -m unittest", "unittest behind uv run"),
    ("unittest", "bare unittest"),
    ("rake test", "bare rake test"),
    ("rails test", "bare rails test"),
    ("bin/rails test", "rails test by path"),
    ("bundle exec rake test", "rake test behind bundle exec"),
    ("bundle exec rspec", "rspec behind bundle exec"),
    ("pest", "bare pest"),
    ("vendor/bin/pest --parallel", "pest with flags only"),
    ("php vendor/bin/pest", "pest behind php"),
    ("php vendor/bin/phpunit", "phpunit behind php"),
    ("cargo nextest r", "`r` is nextest's alias of `run`"),
    ("dart run test", "`dart run test` is `dart test`"),
    (
        "go test",
        "bare go test runs the current package's whole suite",
    ),
    ("go test .", "go test of the current package"),
    ("go test -v", "go test flags only"),
    ("bun test", "bare bun test"),
    ("jest", "bare jest"),
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
    ), // --- Every test runner the adapters know: a filter or path is allowed ---
    ("vitest run -t login", "vitest -t"),
    ("bunx vitest run src/a.test.ts", "vitest path behind bunx"),
    ("npx jest --testNamePattern=login", "jest name pattern"),
    (
        "pnpm exec vitest run src/a.test.ts",
        "vitest path behind pnpm exec",
    ),
    ("mocha --grep login", "mocha --grep"),
    ("rspec spec/a_spec.rb", "rspec path"),
    ("rspec -e login", "rspec -e"),
    ("vendor/bin/phpunit --filter testLogin", "phpunit --filter"),
    ("dotnet test --filter Name~Login", "dotnet --filter"),
    ("mvn test -Dtest=LoginTest", "mvn -Dtest"),
    ("./gradlew test --tests LoginTest", "gradle --tests"),
    ("ctest -R login", "ctest -R"),
    ("swift test --filter LoginTests", "swift --filter"),
    ("dart test test/a_test.dart", "dart path"),
    ("flutter test --name login", "flutter --name"),
    ("mix test test/a_test.exs", "mix path"),
    ("node --test test/a.test.js", "node --test path"),
    ("uv run pytest -k login", "pytest -k behind uv run"),
    ("uv run pytest tests/test_a.py", "pytest path behind uv run"),
    ("go test -run TestLogin ./...", "go test -run"),
    ("dotnet build", "dotnet build is not a test run"),
    ("mvn package", "mvn package is not a test run"),
    // --- Every remaining adapter runner: a filtered run is allowed ---
    ("sbt testOnly com.acme.LoginSpec", "sbt testOnly"),
    (
        r#"sbt "testOnly com.acme.LoginSpec""#,
        "sbt testOnly quoted into one word",
    ),
    (
        "sbt app/testOnly com.acme.LoginSpec",
        "sbt project testOnly",
    ),
    ("sbt compile", "sbt compile is not a test run"),
    (
        "python -m unittest tests.test_login",
        "unittest module path",
    ),
    (
        "python -m unittest discover -s tests",
        "unittest discover -s",
    ),
    ("python3 -m unittest -k login", "unittest -k"),
    (
        "uv run python -m unittest tests.test_login",
        "unittest module behind uv run",
    ),
    ("unittest tests.test_login", "unittest module path"),
    ("rake test TEST=test/a_test.rb", "rake test TEST="),
    (
        "TEST=test/a_test.rb rake test",
        "TEST= assignment before rake narrows the run",
    ),
    (
        "env TEST=test/a_test.rb bundle exec rake test",
        "TEST= assignment behind env and bundle exec",
    ),
    ("rails test test/models/user_test.rb", "rails test file"),
    ("rails test -n test_login", "rails test -n"),
    ("rails test --name=test_login", "rails test --name="),
    (
        "bundle exec rails test test/a_test.rb",
        "rails test behind bundle exec",
    ),
    (
        "ruby -Itest test/a_test.rb",
        "one minitest file is not a full run",
    ),
    (
        "bundle exec rspec spec/a_spec.rb",
        "rspec path behind bundle exec",
    ),
    ("pest tests/Unit/LoginTest.php", "pest path"),
    ("vendor/bin/pest --filter login", "pest --filter"),
    (
        "php vendor/bin/pest --group smoke",
        "pest --group behind php",
    ),
    (
        "php vendor/bin/phpunit --filter testLogin",
        "phpunit behind php",
    ),
    ("phpunit --testsuite unit", "phpunit --testsuite"),
    ("cargo nextest r login::", "nextest r with a filter"),
    (
        "cargo nextest run --partition count:1/2",
        "nextest --partition is a subset",
    ),
    (
        "dart run test test/a_test.dart",
        "`dart run test` with a path",
    ),
    (
        "dart run test --tags=unit",
        "`dart run test` with a tag filter",
    ),
    ("go test ./pkg", "go test of one package"),
    ("go test -v ./pkg", "go test of one package with flags"),
    ("go test -run TestLogin", "go test -run without a package"),
    (
        "go test -bench . -benchtime 1s",
        "go test -bench selects benchmarks",
    ),
    ("bun test src/a.test.ts", "bun test path"),
    (
        "bun test --test-name-pattern=login",
        "bun test name pattern",
    ),
    ("jest --testPathPattern=login", "jest --testPathPattern"),
];
