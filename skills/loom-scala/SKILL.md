---
name: loom-scala
description: Scala language expertise for idiomatic, production-quality code.
triggers:
  - scala
  - scala 3
  - sbt
  - build.sbt
  - scala-cli
  - scalatest
  - munit
  - scalacheck
  - zio
  - cats
  - cats-effect
  - fs2
  - http4s
  - circe
  - doobie
  - given
  - using
  - implicit
  - extension
  - case class
  - sealed trait
  - pattern matching
  - scalafmt
  - scalafix
  - pekko
  - akka
---

# Scala Language Expertise

## Overview

Idiomatic, production-grade Scala 3 on the JVM: sbt builds, the Scala 3 syntax, ADTs and pattern matching, givens and extension methods, typed errors, the cats-effect and ZIO effect systems, and MUnit/ScalaTest testing. Assumes the reader writes Scala already. The content is the part that bites: givens that an import silently misses, `IO.pure` running a side effect once and early, blocking calls on the compute pool, non-exhaustive matches that only warn, and an sbt `testOnly` that exits 0 after running nothing.

## Tooling

sbt is the default build. Keep one sbt shell open (or `sbt --client`) for iterative work: every batch invocation boots a JVM and loads the build before running anything.

| Task | Command |
| --- | --- |
| Full gate | `sbt scalafmtCheckAll scalafmtSbtCheck "scalafixAll --check" test` |
| Compile main and tests | `sbt Test/compile` |
| All tests | `sbt test` |
| One suite | `sbt 'testOnly com.acme.FooSuite'` |
| One ScalaTest test by substring | `sbt 'testOnly com.acme.FooSpec -- -z "rejects symlinks"'` |
| Failed and changed tests only | `sbt testQuick` (`~testQuick` reruns on save) |
| Format | `sbt scalafmtAll scalafmtSbt` |
| Dependency tree | `sbt dependencyTree` (after `addDependencyTreePlugin` in `project/plugins.sbt`) |

```scala
// build.sbt
ThisBuild / scalaVersion := "3.3.4"          // the 3.3.x line is the LTS
ThisBuild / organization := "com.acme"
ThisBuild / semanticdbEnabled := true        // scalafix semantic rules need it

lazy val core = project
  .settings(
    libraryDependencies ++= Seq(
      "org.typelevel" %% "cats-effect" % "3.5.4",
      "org.scalameta" %% "munit" % "1.0.0" % Test,
      "org.typelevel" %% "munit-cats-effect" % "2.0.0" % Test,
    ),
    Test / fork := true,                     // tests in a separate JVM: clean system properties, no classloader leaks
  )

lazy val root = (project in file("."))
  .aggregate(core)                           // commands run at the root reach core
```

- `project/build.properties` pins the sbt version; `project/plugins.sbt` lists plugins.
- `%%` appends the Scala binary version to the artifact (`cats-effect_3`); `%` is for Java libraries. Scala 3 can use Scala 2.13 artifacts through `.cross(CrossVersion.for3Use2_13)`, except those that ship Scala 2 macros.
- Compiler flags: sbt-tpolecat sets strict flags per Scala version (`-deprecation`, `-feature`, `-Wunused:all`, and `-Werror` in CI mode). With warnings as errors, a non-exhaustive match fails the build.
- scalafmt reads `.scalafmt.conf`, which must set `version` and `runner.dialect = scala3`. scalafix semantic rules (`OrganizeImports`, `RemoveUnused`) need SemanticDB enabled as above.
- Scala CLI (the `scala` command since Scala 3.5) runs scripts and single-module projects configured with `//> using` directives. Mill is another build tool. Neither uses `build.sbt`.

## Scala 3 Syntax

```scala
enum Shape:
  case Circle(radius: Double)
  case Rect(width: Double, height: Double)

def area(shape: Shape): Double = shape match
  case Shape.Circle(r)            => math.Pi * r * r
  case Shape.Rect(w, h) if w == h => w * w
  case Shape.Rect(w, h)           => w * h

object ids:
  opaque type UserId = Long                  // a Long at runtime, a distinct type at compile time
  object UserId:
    def apply(value: Long): UserId = value
  extension (id: UserId) def value: Long = id

@main def lineCount(path: String): Unit =
  val lines = scala.util.Using.resource(scala.io.Source.fromFile(path))(_.getLines().size)
  println(s"$path: $lines lines")
```

- Significant indentation replaces braces; `then`/`do` follow `if`/`while`/`for` conditions, and `end` markers close long blocks. Pick one style per codebase and have scalafmt enforce it.
- Top-level definitions replace package objects. `export` forwards selected members of a field, which gives composition without hand-written delegation.
- Union (`String | Int`) and intersection (`A & B`) types; `derives` requests type class derivation (`derives CanEqual`, circe's `derives Codec.AsObject`).
- `opaque type` gives a distinct type with no runtime cost. `AnyVal` value classes box inside collections and generic code; opaque types never box beyond their underlying type.
- Migrating from 2.13: compile with `-source:3.0-migration -rewrite` to fix syntax. `implicit` definitions still compile in Scala 3 and interoperate with givens.

## ADTs and Pattern Matching

```scala
sealed trait Command
object Command:
  final case class Upload(source: String, target: String) extends Command
  final case class Delete(target: String) extends Command
  case object Flush extends Command

def run(cmd: Command): Either[String, Unit] = cmd match
  case Command.Upload(src, dst) if dst.isBlank => Left(s"empty target for $src")
  case Command.Upload(src, dst)                => upload(src, dst)
  case Command.Delete(target)                  => delete(target)
  case Command.Flush                           => Right(())
```

- `sealed` traits and `enum`s let the compiler check exhaustiveness. The check only warns, so build with `-Werror`. The compiler cannot see through guards: keep an unguarded case for every variant.
- `final case class` keeps data types closed. Case classes give structural `equals`/`hashCode`, `copy`, and an extractor for patterns.
- Type patterns are erased: `case xs: List[String]` matches any `List` and only emits an unchecked warning. Match on the elements, or keep the type in the ADT.
- A value no case matches throws `MatchError` at runtime. `collect { case ... }` and other `PartialFunction` uses make partiality explicit.
- Custom extractors: `object Email { def unapply(s: String): Option[(String, String)] = ... }` makes `case Email(user, domain) =>` work on plain strings.
- Scala 3 deprecates non-local `return` from inside a lambda. Use `scala.util.boundary` (3.3):

```scala
import scala.util.boundary, boundary.break

def firstNegative(xs: List[Int]): Option[Int] =
  boundary:
    for x <- xs do
      if x < 0 then break(Some(x))
    None
```

## Givens, Using and Extension Methods

```scala
trait Show[A]:
  extension (a: A) def show: String

object Show:                                   // the companion is in the implicit scope of Show[X]
  given Show[Int] with
    extension (a: Int) def show: String = a.toString

  given [A](using s: Show[A]): Show[List[A]] with
    extension (as: List[A]) def show: String = as.map(a => s.show(a)).mkString("[", ", ", "]")

def describe[A: Show](a: A): String = s"value: ${a.show}"   // the context bound puts Show[A] in scope

describe(42)              // Show[Int] found in the companion, no import
describe(List(1, 2, 3))   // Show[List[Int]] built from Show[Int]
```

- Put instances in the companion of the type class or of the data type: that is the implicit scope, searched without imports. Instances in unrelated objects (orphans) need explicit imports and invite ambiguity.
- `import a.*` does not import `given` definitions. Use `import a.given` or `import a.{*, given}`. Scala 2 `implicit` definitions, including most of cats' syntax, still arrive with `*`.
- Extension syntax on a bare value (`42.show` outside any `Show` context) searches the implicit scope of the receiver's type, which does not include `Show`'s companion; bring the instances into lexical scope with `import Show.given`.
- Two eligible givens of the same type fail with an ambiguity error. Resolve by specificity or by moving the fallback into a lower-priority parent trait of the companion.
- `using` parameters carry context (an `ExecutionContext`, a transaction, a config). Passing ordinary dependencies implicitly hides them from readers; use constructor parameters.
- Implicit conversions (`given Conversion[A, B]`) need a language import and surprise readers. Extension methods cover the legitimate cases.
- Automatic derivation of deep type class trees (circe's fully automatic mode) is slow to compile. Semi-automatic `derives` on each type keeps compile times flat.

## Errors

```scala
enum ConfigError:
  case Missing(key: String)
  case Invalid(key: String, value: String)

def port(env: Map[String, String]): Either[ConfigError, Int] =
  for
    raw  <- env.get("PORT").toRight(ConfigError.Missing("PORT"))
    port <- raw.toIntOption.filter(p => p > 0 && p < 65536).toRight(ConfigError.Invalid("PORT", raw))
  yield port

def readConfig(path: Path): Either[Throwable, String] =
  Try(Files.readString(path)).toEither          // wrap the throwing Java call once, at the edge
```

- `Either[E, A]` with an error ADT for expected failures, `Option` for absence, `Try` only to capture exceptions from Java or legacy APIs, converted to `Either` at the edge. For-comprehensions chain all three.
- Throw for bugs. Inside an effect type, failures travel in the effect (`IO.raiseError`, ZIO's error channel).
- Catch with `case NonFatal(e) =>`. `case e: Throwable` also catches `InterruptedException` and VM errors.
- `scala.util.Using.resource(open())(use)` and `Using.Manager` close `AutoCloseable` resources on every path.

## Effects: Cats Effect and ZIO

Pick one effect system per codebase. `Future` is eager, memoized and not cancellable; it suits simple async glue, and blocking code inside it belongs in `blocking {}`.

```scala
import cats.effect.{IO, IOApp, Resource}

object Main extends IOApp.Simple:
  def fetch(id: Long): IO[String] =
    IO.blocking(legacyClient.get(id))                        // blocking call on the blocking pool

  val run: IO[Unit] =
    Resource.fromAutoCloseable(IO(openDatabase())).use { db =>
      IO.parTraverseN(4)((1L to 20L).toList)(fetch)          // at most 4 in flight
        .flatMap(pages => IO.blocking(db.saveAll(pages)))
    }
```

- An `IO` value describes a computation; nothing runs until the runtime executes it. `IO.pure(expr)` evaluates `expr` once, at construction. Side effects go in `IO(...)`/`IO.delay`.
- Blocking calls go in `IO.blocking`, or `IO.interruptible` to allow cancellation by thread interrupt. A blocking call in plain `IO` stalls one of the compute threads, of which there is one per core.
- `Resource` for anything with a release step; `use` releases on success, error and cancellation.
- `Ref` for shared state, `Deferred` for a one-shot signal, `Queue` for handoff between fibers. A fiber from `start` that nobody joins leaks when its parent is cancelled; use `background` (a `Resource`) or a `Supervisor`.
- `unsafeRunSync()` belongs at the program edge only. Inside `IO` code it blocks a compute thread and can deadlock.

```scala
import zio.*

trait UserRepo:
  def find(id: Long): IO[RepoError, Option[User]]

final case class UserService(repo: UserRepo):
  def name(id: Long): IO[RepoError, String] =
    repo.find(id).someOrFail(RepoError.NotFound(id)).map(_.name)

object UserService:
  val layer: ZLayer[UserRepo, Nothing, UserService] =
    ZLayer.fromZIO(ZIO.service[UserRepo].map(UserService(_)))
```

- `ZIO[R, E, A]` carries an environment, a typed error and a result. Expected failures go in `E`; defects (`ZIO.die`, exceptions thrown inside `ZIO.succeed`) are for bugs. `orDie` turns a failure into a defect, `refineOrDie` keeps the errors callers can handle.
- `ZIO.succeed` takes its argument by name, so the effect is deferred; code that can throw belongs in `ZIO.attempt`, blocking code in `ZIO.attemptBlocking`.
- `ZLayer` wires dependencies; `ZIO.acquireRelease` with `ZIO.scoped` manages resources; `ZIO.foreachPar(xs)(f).withParallelism(n)` bounds fan-out.

## Collections

```scala
val totals: Map[String, BigDecimal] =
  orders.groupMapReduce(_.customer)(_.amount)(_ + _)                  // one pass, duplicates summed

val firstLarge = orders.view.filter(_.amount > 1000).map(_.id).headOption   // stops at the first match
```

- Immutable by default. `List` prepends in O(1), but indexing and `:+` are O(n); `Vector` gives near-constant indexing and append. Build locally with `ListBuffer`/`ArrayBuffer` and return an immutable result.
- `.view` makes a chain lazy; `Iterator` is single-pass; `LazyList` memoizes.
- `Map.mapValues` and `filterKeys` return lazy views in 2.13 and 3 that recompute on every access (and are deprecated). Write `.view.mapValues(f).toMap`.
- `toMap` on pairs keeps the last value for a duplicate key without warning. `groupBy`, `groupMap` and `groupMapReduce` keep or combine them.
- `Array` equality is reference equality; compare with `sameElements` or use `ArraySeq`.
- `foldLeft` is stack-safe on `List`; deep `foldRight` over a large `List` is where stack overflows come from in older code.

## Testing

```scala
package com.acme.spool

import java.nio.file.{Files, Path}

class SymlinkedSpoolSuite extends munit.FunSuite:
  val dirs = FunFixture[Path](
    setup = _ => Files.createTempDirectory("spool"),
    teardown = dir => deleteTree(dir),
  )

  dirs.test("rejects a spool directory that is a symlink") { root =>
    val outside = Files.createTempDirectory("outside")
    val spool = Files.createSymbolicLink(root.resolve("spool"), outside)
    intercept[SecurityException](SpoolWriter(spool).write("a.json", "{}"))
  }
```

- MUnit (`munit.FunSuite`): `test("name") { ... }`, `assertEquals(obtained, expected)` with a diff on failure, `intercept[E]`, `FunFixture` for per-test setup. `munit-cats-effect`'s `CatsEffectSuite` runs `IO` tests (`assertIO(io, expected)`).
- ScalaTest: styles (`AnyFunSuite`, `AnyFlatSpec`, `AnyWordSpec`) with `Matchers` (`result shouldBe 3`, `an[IllegalArgumentException] should be thrownBy f()`); `AsyncFunSuite` for `Future` code.
- ZIO Test: `object UserServiceSpec extends ZIOSpecDefault` with `suite(...)(test(...) { ... assertTrue(...) })`; `TestClock` controls time.
- Property tests: ScalaCheck `forAll`, through `munit-scalacheck` or ScalaTest's `ScalaCheckPropertyChecks`.
- Without forking, sbt runs suites in parallel inside its own JVM. With `Test / fork := true` they run in one separate JVM, one suite after another unless `testGrouping` splits them. Suites that share files, ports or global state need `Test / parallelExecution := false` or isolation.

## Loom Test Runner Adapter

**Adapter.** Scala uses `sbt`. A directory with `build.sbt` is kind `scala`, and the runner for kind `scala` is always `sbt`; `loom project detect` prints it per package. Mill and Scala CLI projects have no `build.sbt`, so detection does not see them as Scala.

```text
ingest  kinds=scala  runner=sbt  skills=loom-scala
```

**Single-test command.** loom runs it with the package directory, the one holding `build.sbt`, as the working directory:

```bash
sbt 'testOnly {test}'
```

**The `test` field.** The fully qualified suite name, for example `com.acme.spool.SymlinkedSpoolSuite`; for a ZIO Test `object`, the object's name as written in source. The command selects a whole suite and passes no per-test filter, so every test in that suite runs.

**No-match behaviour.** D5 marks `sbt` "documented": no fixture was captured, and loom's `sbt` parser was written from sbt's documented output. When the name matches no suite, sbt prints `No tests to run for Test / testOnly` and exits 0, so the exit code alone would read a misspelled name as a pass. loom reads the runner's summary instead: a name that selects nothing classifies as `NotSelected`, which fails the freeze ("the runner did not select the test") and fails the completion check ("contract test not selected").

**Writing contract tests.** Test sources live under `src/test/scala/` of the subproject that owns the code, for example `core/src/test/scala/com/acme/spool/SymlinkedSpoolSuite.scala` for `lazy val core = project`. Because the adapter selects suites:

- write one suite per contract, holding only that contract's test, and name it after the contract;
- keep the `package` clause equal to the directory path; Scala does not enforce it, and a mismatch makes the suite name differ from what the path suggests;
- make sure the root project aggregates the subproject (`.aggregate(core)`); `testOnly` at the root never reaches an unaggregated subproject. A build with no explicit root project gets a default root that aggregates every subproject.

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: core/src/test/scala/com/acme/spool/SymlinkedSpoolSuite.scala
    test: com.acme.spool.SymlinkedSpoolSuite
    scenario: the spool directory is a symlink to a directory outside the data root
    rejects: a writer that follows the symlink and writes files outside the data root
```

**Build failures.** A contract suite that does not compile yet counts as red at freeze time: loom classifies the compiler failure as `BuildFailed`. sbt compiles a subproject's whole `Test` configuration together, so one uncompilable contract suite makes every suite in that subproject `BuildFailed` until the implementation exists. Freeze and completion run each command under a 300-second limit, and every run starts a fresh sbt: run the single-test command once before freezing so dependency resolution and the first compile happen outside the timed run. A contract that needs a new test dependency edits `build.sbt`, which must match one of the stage's `harness` globs.

## Anti-Patterns

```scala
// Partial accessors throw on the empty case
val user = users.find(_.id == id).get                     // BAD: NoSuchElementException
val user = users.find(_.id == id).toRight(UserNotFound(id))   // GOOD: the caller handles absence

// Catching everything
try risky() catch { case e: Throwable => log(e) }         // BAD: also catches InterruptedException, VM errors
try risky() catch { case NonFatal(e) => log(e) }          // GOOD

// Side effect inside IO.pure runs once, when the value is built
val tick = IO.pure(println("tick"))                       // BAD
val tick = IO.println("tick")                             // GOOD: runs every time tick runs

// Blocking inside IO on the compute pool
IO(statement.executeQuery(sql))                           // BAD: stalls a compute thread
IO.blocking(statement.executeQuery(sql))                  // GOOD
```

Quick swaps: `null` from Java → `Option(javaValue)`; `.head`/`.last` on a possibly empty collection → `headOption`/`lastOption`; `return` inside a lambda → `boundary`/`break` or a fold; `Await.result` in `Future` code → `flatMap`/`map`; a `var` holding a mutable collection shared between threads or fibers → `Ref` or `AtomicReference`; `String` or `Long` ids → opaque types; `asInstanceOf` → a pattern match; implicit conversions → extension methods; `List[Any]` → an ADT; `==` between unrelated types (compiles by default) → `-language:strictEquality` with `derives CanEqual`.

## Expert Practices

### Equality and Variance

- Universal equality lets `1 == "1"` compile (and return `false`). `-language:strictEquality` plus `derives CanEqual` on your types turns such comparisons into compile errors.
- Variance: `+A` for producers and immutable containers, `-A` for consumers (function inputs, encoders). Mutable containers stay invariant; that is why `Array[A]` is invariant.
- Case classes holding functions or arrays have `equals` by reference for those fields.

### Security

- The JVM rules apply: no Java deserialization of untrusted bytes, XML parsers with DTDs disabled, `SecureRandom` for tokens, normalized paths checked against their base. Pekko keeps Java serialization off by default, as Akka does since 2.6; leave it off.
- SQL interpolators: doobie's `sql"... where name = $name"` and Slick's `sql"... $name"` bind parameters. doobie's `Fragment.const` and Slick's `#$name` splice raw text and are injectable.
- JSON codecs (circe, jsoniter-scala) decode into known types. Keep class names out of the payload; model polymorphism as a sealed ADT with a discriminator field.
- `sys.process` with a single string splits on whitespace (`"tar -xf " + file`). Pass a `Seq` of arguments.

### Performance

- `@tailrec` makes the compiler verify that a self-recursive call is in tail position; it compiles to a loop.
- Generic code over `Int`, `Long` or `Double` boxes every element; Scala 3 does not specialize. Hot numeric loops use `Array[Int]` or `IArray`.
- Appending to a `List` in a loop is quadratic. Prepend and reverse once, or build with `ListBuffer`/`VectorBuilder`.
- Compile time is a cost: automatic derivation, large macros and very large files slow every build. Split modules along dependency lines so incremental compilation recompiles less.
- Profile with JFR or async-profiler, and benchmark with sbt-jmh; timing loops in tests measure JIT warm-up.

## Verification Checklists

**Before marking Scala work done:**

- [ ] `sbt scalafmtCheckAll "scalafixAll --check" test` passes, with warnings as errors (sbt-tpolecat or `-Werror`)
- [ ] New behaviour has tests; the new suites appear in the run summary with a non-zero count
- [ ] Every `match` over an ADT is exhaustive without guards hiding a variant; no type patterns on erased type arguments
- [ ] No `.get`, `.head` or `asInstanceOf` on values that can be absent or of another type
- [ ] Expected failures are typed (`Either`, the effect's error channel); catches use `NonFatal`
- [ ] Givens live in companions or are imported with `given`; no new implicit conversions
- [ ] Raw SQL splices (`Fragment.const`, `#$`) never see untrusted input

**Effects and concurrency review:**

- [ ] Side effects are suspended (`IO(...)`, `ZIO.attempt`); no `IO.pure` around effects
- [ ] Blocking calls in `IO.blocking`/`ZIO.attemptBlocking`; fan-out bounded (`parTraverseN`, `withParallelism`)
- [ ] Resources acquired through `Resource`/`ZIO.acquireRelease`; no fiber started without an owner
- [ ] No `unsafeRunSync`, `Await.result` or mutable shared state outside `Ref` inside effect code
- [ ] Suites that share files, ports or global state run isolated (`Test / parallelExecution` or forking)
