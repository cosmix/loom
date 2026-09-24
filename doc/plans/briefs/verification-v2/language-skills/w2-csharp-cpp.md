# language-skills / W2 — .NET and C++ skills

Read first: `doc/plans/briefs/verification-v2/language-skills/SPEC.md` (binding shape and the
adapter section); `doc/plans/briefs/verification-v2/DESIGN.md` D5, D6, D7;
`skills/loom-python/SKILL.md` and `skills/loom-rust/SKILL.md` as shape references.

## Files you own

`skills/loom-csharp/SKILL.md`, `skills/loom-cpp/SKILL.md` (all new).

## Scope per skill

- `loom-csharp`: the dotnet CLI, nullable reference types, records, async/await and `ConfigureAwait`, LINQ pitfalls, ASP.NET Core minimal APIs, xUnit/NUnit. Adapter `dotnet-test`.
- `loom-cpp`: CMake, C++20/23 features, RAII and ownership, `std::span`/`string_view` lifetimes, undefined behaviour and sanitizers, GoogleTest/Catch2 with CTest. Adapter `ctest` (note the `cmake --build build &&` prefix loom adds so build failures surface).

Each skill follows SPEC.md's section order, is 350–550 lines, and has the exact heading
`## Loom Test Runner Adapter`. Commands, `test` value formats, detection rules and no-match
behaviour come from DESIGN D5/D7 verbatim. Where D5 marks a runner "documented", say that
loom's parser for it was written from the runner's documented output.

## Proof (one command, once)

`rg -c "^## " skills/loom-csharp/SKILL.md skills/loom-cpp/SKILL.md`
(each file shows the eight required headings or more).

## Report

Files created with their line counts; any D5/D7 fact you found wrong for the runner, which the
main agent records in loom memory.
