# runner-adapters / W3 — `loom project detect`

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D7. Knowledge:
`patterns/cli-process-and-conventions.md` (the CLI registration section);
`mistakes/pinned-literals-ledgers-and-wiring.md` "The Maintainability Ledger Is EXACT-Match, Not
a Ceiling" (the `dispatch.rs dispatch 205` example: a ledgered top-level match cannot take one
more arm).

W2 writes `ProjectProfile::package_details()` in parallel. Its shape is pinned in W2's brief:
`PackageDetail { path: PathBuf, kinds: Vec<String>, runner: Option<&'static str>, skills: Vec<String> }`,
`Serialize`.

## Files you own

`loom/src/cli/types.rs` (369 lines; stays ≤ 400), `loom/src/cli/types_project.rs` (new),
`loom/src/cli/dispatch.rs` (`dispatch` ledgered at 86 lines), `loom/src/commands/mod.rs`,
`loom/src/commands/project.rs` (new), `loom/tests/integration/project_detect.rs` (new),
`loom/tests/integration/mod.rs`.

## Tasks

1. `types_project.rs`: `ProjectCommands::Detect { path: Option<PathBuf>, #[arg(long)] json: bool }`
   with doc comments that become `--help` text. `types.rs`: a `Project { #[command(subcommand)] command: ProjectCommands }`
   variant in `Commands`, re-exported like the other `types_*` files.
2. `dispatch.rs`: route `Commands::Project` to `commands::project::execute`. `dispatch` must
   keep EXACTLY 86 lines. This stage runs in parallel with schema-v2, which owns the ledger, so
   the ledger file is not edited here. Move an existing arm's body into a helper so the function
   length stays 86.
3. `commands/project.rs`: `execute(path, json)`. Discover from `path` or the current directory
   (`ProjectProfile::discover`), then print exactly the D7 human format or one compact JSON line
   (`serde_json::to_string`), with `"runner":null` for unsupported packages.
4. `tests/integration/project_detect.rs` (declare it in `tests/integration/mod.rs`): uses the
   integration helpers' `loom_cmd()`.

## Named test (binding)

- `project_detect_json_reports_packages`: a temp checkout (`git init`) with `rust/Cargo.toml`
  (a minimal crate) and `web/package.json` (`devDependencies.vitest`, plus `tsconfig.json`).
  `loom project detect --json <root>` prints JSON whose `packages` contains `rust` with runner
  `cargo-test` and skill `loom-rust`, and `web` with runner `vitest` and skill
  `loom-typescript`. Parse the JSON; do not substring-match it.

## Proof (one command, once, after W2 reports)

`cargo test --manifest-path loom/Cargo.toml --test integration -- project_detect_json_reports_packages`

## Report

Files changed; the exact `dispatch` line count (must be 86) and `types.rs` line count; the proof
result.
