# flutter-test fixture

Source project: scratch `runner-projects/flutter-test/` (`flutter create
--project-name fixture_flutter --platforms=linux flutter-test`), Flutter
3.24.1 / Dart 3.5.1. Project creation and the first `flutter test` run
both completed in single-digit seconds (no >5 min timeout hit).

`test/fixture_test.dart` (uses `package:flutter_test/flutter_test.dart`,
plain `test()`/`expect()`, no widget pumping):

- `alpha_passes` (passes)
- `beta_fails` (fails: `expect(add(2, 2), equals(5), reason: 'deliberate failure')`)
- `gamma_passes` (passes)

The template's own `test/widget_test.dart` was left untouched and unused —
every scenario targets `test/fixture_test.dart` explicitly, so it never runs.

The adapter's current filter command form is `flutter test <file> --name
'^<full runner-visible name>$'` with regex metacharacters escaped.

`.nocolor` variants add `--no-color`; bytes were identical to the default
run (1001 bytes each) — unlike plain `dart test` on the same flag,
`flutter test`'s wrapped output carries no ANSI escapes by default here.

Notable: `no-match` (`--plain-name 'delta_missing'`) exits **1** with
`No tests match "delta_missing".` on stderr — same message as `dart test`
but a different exit code (dart test: 79, flutter test: 1).

`collision_test.dart` checks the exact selector against overlapping names.
Run it from a generated Flutter package with cached dependencies using
`flutter test --no-pub test/collision_test.dart --name '^suite value \(a\)\+$'`.
It selects one passing test; with `VER01_SKIP_EXACT=1`, Flutter 3.24.1
reports `+0 ~1: All tests skipped.` and exits 0. The prefix and suffix
siblings fail if selected, so neither can satisfy the missing exact test.
