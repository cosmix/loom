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

Filter command form: `flutter test <file> --plain-name '<name>'`.

`.nocolor` variants add `--no-color`; bytes were identical to the default
run (1001 bytes each) — unlike plain `dart test` on the same flag,
`flutter test`'s wrapped output carries no ANSI escapes by default here.

Notable: `no-match` (`--plain-name 'delta_missing'`) exits **1** with
`No tests match "delta_missing".` on stderr — same message as `dart test`
but a different exit code (dart test: 79, flutter test: 1).
