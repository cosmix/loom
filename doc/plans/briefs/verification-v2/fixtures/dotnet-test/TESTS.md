# dotnet-test fixture

Source project: scratch `runner-projects/dotnet-test/` — `dotnet new xunit -n FixtureDotnet`
(xUnit 3 via .NET SDK 10.0.112), namespace `FixtureDotnet`.

`UnitTest1.cs`, `class FixtureTests`:

- `AlphaPasses` (passes)
- `BetaFails` (fails: `Assert.Equal(5, Add(2, 2))`)
- `GammaPasses` (passes)

Filter command form:
`dotnet test --filter "FullyQualifiedName=<Namespace.Class.Method>"`, i.e.
`FullyQualifiedName=FixtureDotnet.FixtureTests.AlphaPasses`.

`.nocolor` variants add `DOTNET_NOLOGO=1 NO_COLOR=1`; output was
byte-for-byte equal (688 vs 687 bytes, a trailing-newline artifact) to the
default run — dotnet test already emits no ANSI color on a redirected pipe.

Notable: `no-match` (`FullyQualifiedName=...DeltaMissing`) exits **0** with
`No test matches the given testcase filter ...` printed to stdout (not
stderr) — another zero-exit-on-no-match runner.
