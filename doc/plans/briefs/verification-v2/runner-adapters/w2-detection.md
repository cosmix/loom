# runner-adapters / W2 — detection and runner selection

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D7. Code:
`loom/src/skills/project.rs` (47 lines), `project/markers.rs` (160; `FILE_MARKERS` at 6-73,
`detect_dependencies` at 96-125 is the only `package.json` parser), `project/scan.rs`,
`project/scope.rs`, `skills/recommend.rs` (`resolve_skill`).

## Files you own

`loom/src/skills/project.rs`, `loom/src/skills/project/markers.rs`,
`loom/src/skills/project/runners.rs` (new), `loom/src/skills/project/tests.rs`,
`loom/src/skills/project/runners_tests.rs` (new), `loom/src/skills/recommend.rs`.

## Tasks

1. `markers.rs`: the new kinds of D7. `csharp` needs an extension-based marker (`*.csproj`,
   `*.sln` in the directory). Add it as a small separate check, not by listing names.
   `javascript` is `package.json` present and `tsconfig.json` absent.
2. `runners.rs`: `pub fn detect_runner(package_dir: &Path, checkout_root: &Path, kinds: &[String]) -> Option<&'static str>`,
   the D7 table in order, returning adapter names exactly as D5 spells them. Parse
   `package.json` with a shared reader: move the existing `read_json` from `markers.rs` into a
   helper both files call rather than duplicate it. Look for `pytest` in `requirements*.txt` and
   `pyproject.toml`'s dependency arrays, `rspec` in `Gemfile`, `pestphp/pest` in
   `composer.json`, `flutter` in `pubspec.yaml`. Plain substring reads are enough; no new
   dependency.
3. `project.rs`: `ProjectProfile::package_details(&self) -> Vec<PackageDetail>` with
   `pub struct PackageDetail { pub path: PathBuf, pub kinds: Vec<String>, pub runner: Option<&'static str>, pub skills: Vec<String> }`,
   deriving `Serialize`. Skills per D7 (`loom-<kind>`, `javascript` ⇒ `loom-typescript`).
   Infrastructure kinds (docker, kubernetes, ...) keep today's skill resolution and never get a
   runner.
4. `recommend.rs`: `resolve_skill` maps kind `javascript` to `loom-typescript`, so skill
   recommendation works for plain JavaScript packages.

## Named tests (binding), in `project/runners_tests.rs` (declared from `project.rs` or

`runners.rs` with `#[cfg(test)] #[path = ...]`)

- `detects_runner_for_each_ecosystem`: one temp package per D7 row (minimal marker files) →
  the expected adapter name, including `cargo-nextest` with `.config/nextest.toml`, `pytest`
  with `conftest.py`, `unittest` without it, `gradle` versus `maven`, `rspec` versus
  `minitest`, `pest` versus `phpunit`, `flutter-test` versus `dart-test`.
- `javascript_package_maps_to_typescript_skill`: `package.json` without `tsconfig.json` → kind
  `javascript`, skill `loom-typescript`.
- `vitest_dependency_wins_over_bun_lockfile`: `devDependencies.vitest` plus `bun.lock` →
  `vitest` (this repository's `web/` is that case).

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib skills::`

## Report

Files changed; the `PackageDetail` definition as written (W3 prints it); the proof result.
