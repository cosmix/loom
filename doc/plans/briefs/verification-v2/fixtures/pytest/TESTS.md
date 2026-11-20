# pytest fixture

Source project: scratch `runner-projects/pytest/test_fixture.py`.

- `test_alpha_passes` (passes)
- `test_beta_fails` (fails: `assert add(2, 2) == 5, "deliberate failure"`)
- `test_gamma_passes` (passes)

Two filter forms captured per scenario:

- `python3 -m pytest '<file>::<name>' -q` — base scenario files
  (`one-pass`, `one-fail`, `no-match`, `suite`)
- `python3 -m pytest <file> -q -k <name>` — `.k` suffixed files
  (`one-pass.k`, `one-fail.k`, `no-match.k`; no `suite.k`, identical to `suite`)

`.nocolor` variants use `--color=no`, captured for the `::` form only.

Notable exit codes on `no-match` (name `test_delta_missing`, which does not
exist) — **neither form exits 0**:

- `::` form: exit **4** (pytest usage error: "not found" — stderr has an
  `ERROR:` line), because `test_fixture.py::test_delta_missing` names a
  nonexistent node id directly.
- `-k` form: exit **5** (pytest "no tests ran" — `-k` is a substring filter
  over the whole file, so an unmatched pattern is not a usage error).
