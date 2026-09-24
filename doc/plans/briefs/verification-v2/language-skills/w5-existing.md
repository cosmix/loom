# language-skills / W5 — adapter sections in existing skills, catalog table

Read first: `doc/plans/briefs/verification-v2/language-skills/SPEC.md` (the adapter section);
`doc/plans/briefs/verification-v2/DESIGN.md` D5, D6, D7.

## Files you own

`skills/loom-rust/SKILL.md`, `skills/loom-golang/SKILL.md`, `skills/loom-python/SKILL.md`,
`skills/loom-typescript/SKILL.md`, `skills/loom-skills/SKILL.md`.

## Tasks

1. Add `## Loom Test Runner Adapter` to each of the four language skills, placed right after the
   skill's testing section (`## Testing` in loom-rust, `## Testing with pytest` in loom-python;
   in loom-golang and loom-typescript, after the section that discusses tests: `go test` in
   Foundations/Verification, and "Toolchain: Bun + Vite + Oxc" which contrasts `bun test` and
   Vitest). Contents per SPEC.md:
   - loom-rust: `cargo-test`, `cargo-nextest`;
   - loom-golang: `go-test`;
   - loom-python: `pytest`, `unittest`;
   - loom-typescript: `vitest`, `jest`, `mocha`, `bun-test`, `node-test`, and that the D7 order
     picks among them.

   Edit nothing else in those skills.
2. `skills/loom-skills/SKILL.md`: add a catalog table row for each of the ten new skills, in the
   table's alphabetical position and format, with a one-line description in the style of the
   neighbouring rows. Example row: ``| `loom-java` | Idiomatic Java: JUnit 5, Gradle/Maven, Spring Boot, records |``.
   The ten names: `loom-java`, `loom-kotlin`, `loom-scala`, `loom-csharp`, `loom-cpp`,
   `loom-ruby`, `loom-php`, `loom-elixir`, `loom-swift`, `loom-dart`.

## Proof (one command, once)

`rg -c "^## Loom Test Runner Adapter$" skills/loom-rust/SKILL.md skills/loom-golang/SKILL.md skills/loom-python/SKILL.md skills/loom-typescript/SKILL.md`

## Report

Files changed; the ten catalog rows as written.
