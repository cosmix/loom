# dart-test fixture

Source project: scratch `runner-projects/dart-test/` (`pubspec.yaml` with
`dev_dependencies: test: ^1.25.0`, resolved via `dart pub get` to `test 1.26.3`).

`test/fixture_test.dart`:

- `alpha_passes` (passes)
- `beta_fails` (fails: `expect(add(2, 2), equals(5), reason: 'deliberate failure')`)
- `gamma_passes` (passes)

Filter command form: `dart test <file> --plain-name '<name>'`.

`.nocolor` variants add `--no-color`; default output DOES carry ANSI color
codes even on the redirected pipe (866 bytes vs 801 for `one-pass.stdout`
vs `.nocolor`), unlike node/bun/vitest/jest/mocha which auto-detect the
non-TTY and suppress color already.

Notable: `no-match` (`--plain-name 'delta_missing'`) exits **79** (not 0,
not 1) with `No tests match "delta_missing".` on stderr — a distinct exit
code from every other runner captured in this fixture set.
