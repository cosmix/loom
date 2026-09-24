# language-skills / W1 — JVM skills

Read first: `doc/plans/briefs/verification-v2/language-skills/SPEC.md` (binding shape and the
adapter section); `doc/plans/briefs/verification-v2/DESIGN.md` D5, D6, D7;
`skills/loom-python/SKILL.md` and `skills/loom-rust/SKILL.md` as shape references.

## Files you own

`skills/loom-java/SKILL.md`, `skills/loom-kotlin/SKILL.md`, `skills/loom-scala/SKILL.md` (all new).

## Scope per skill

- `loom-java`: Maven and Gradle, JUnit 5, records, sealed types, streams, virtual threads, Spring Boot essentials. Adapters `gradle` and `maven`.
- `loom-kotlin`: Gradle Kotlin DSL, coroutines and structured concurrency, null safety, data/sealed classes, Ktor and Spring with Kotlin, kotest/JUnit 5. Adapter `gradle` (Kotlin projects build with Gradle; name `maven` where a Maven Kotlin build applies).
- `loom-scala`: sbt, Scala 3 syntax, ADTs and pattern matching, implicits/givens, cats-effect or ZIO basics, ScalaTest/MUnit. Adapter `sbt`.

Each skill follows SPEC.md's section order, is 350–550 lines, and has the exact heading
`## Loom Test Runner Adapter`. Commands, `test` value formats, detection rules and no-match
behaviour come from DESIGN D5/D7 verbatim. Where D5 marks a runner "documented", say that
loom's parser for it was written from the runner's documented output.

## Proof (one command, once)

`rg -c "^## " skills/loom-java/SKILL.md skills/loom-kotlin/SKILL.md skills/loom-scala/SKILL.md`
(each file shows the eight required headings or more).

## Report

Files created with their line counts; any D5/D7 fact you found wrong for the runner, which the
main agent records in loom memory.
