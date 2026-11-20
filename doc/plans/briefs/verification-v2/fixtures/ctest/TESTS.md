# ctest fixture

Source project: scratch `runner-projects/ctest/` — a CMake C project with
three separate one-file executables, one per test (`add_test()` in ctest
maps a named test straight to a process's exit code):

- `alpha_passes.c` → executable/test `alpha_passes` (passes, `assert(add(2,2)==4)`)
- `beta_fails.c` → executable/test `beta_fails` (fails: `assert(add(2,2)==5)` aborts)
- `gamma_passes.c` → executable/test `gamma_passes` (passes, `assert(add(1,1)==2)`)

Configured with `/usr/bin/cmake -S . -B build`, built with
`/usr/bin/cmake --build build`.

Filter command form: `ctest --test-dir build -R '^<name>$' --output-on-failure`
(used the full absolute path `/usr/bin/ctest`, see NOTES.md for why).

`build-error` scenario: ctest itself never compiles anything (unlike
cargo/go/dotnet/dart, whose test command IS the build command), so a
compile error only surfaces through the paired build step. The command
recorded for this scenario is the realistic pipeline a CI script runs:
`cmake --build build && ctest --test-dir build -R '^alpha_passes$' --output-on-failure`.
It fails at the `cmake --build` stage (exit 2, compiler diagnostics on
stderr) and `&&` short-circuits, so ctest never runs.

No `.nocolor` variants captured: ctest's default output has no ANSI color
codes to begin with (no `--no-color`/`NO_COLOR` support needed).

Notable: `no-match` (`-R '^delta_missing$'`) exits **0** with
`No tests were found!!!` printed to stderr — zero-exit on no match, same
family as cargo/go/node/vitest/jest/mocha above.
