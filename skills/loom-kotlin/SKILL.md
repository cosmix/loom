---
name: loom-kotlin
description: Kotlin language expertise for idiomatic, production-quality code.
triggers:
  - kotlin
  - kt
  - kts
  - build.gradle.kts
  - gradle kotlin dsl
  - gradle
  - coroutines
  - kotlinx
  - suspend
  - flow
  - stateflow
  - ktor
  - kotest
  - mockk
  - junit
  - detekt
  - ktlint
  - data class
  - sealed class
  - value class
  - null safety
  - kotlinx.serialization
  - spring kotlin
---

# Kotlin Language Expertise

## Overview

Idiomatic, production-grade Kotlin 2.x on the JVM: null safety, data and sealed classes, coroutines with structured concurrency, Flow, Gradle Kotlin DSL builds, Ktor, Spring with Kotlin, and JUnit 5/kotest testing. Assumes the reader writes Kotlin already. The content is the part that goes wrong in production: `runCatching` swallowing cancellation, `GlobalScope` leaks, blocking calls on the wrong dispatcher, platform types from Java, final classes breaking Spring proxies.

## Tooling

Gradle with the Kotlin DSL is the default build; the K2 compiler is the default since Kotlin 2.0. Use the wrapper (`./gradlew`) and keep versions in the catalog (`gradle/libs.versions.toml`).

| Task | Command |
| --- | --- |
| Full gate | `./gradlew check` (tests plus detekt/ktlint when their plugins are applied) |
| Compile main and tests | `./gradlew testClasses` |
| Unit tests | `./gradlew test` |
| One test | `./gradlew test --tests 'com.acme.FooTest.bar'` |
| Static analysis | `./gradlew detekt` |
| Format | `./gradlew ktlintFormat` (or `spotlessApply` with ktlint or ktfmt) |
| Dependency tree | `./gradlew dependencies --configuration runtimeClasspath` |

```kotlin
// build.gradle.kts
plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.detekt)
    alias(libs.plugins.ktlint)
}

kotlin {
    jvmToolchain(21)
    compilerOptions {
        allWarningsAsErrors = true
        freeCompilerArgs.add("-Xjsr305=strict")   // Java nullability annotations become Kotlin types
    }
}

dependencies {
    implementation(libs.kotlinx.coroutines.core)
    testImplementation(kotlin("test"))            // kotlin.test on JUnit 5 once useJUnitPlatform() is set
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockk)
}

tasks.test { useJUnitPlatform() }
```

- Share build logic across subprojects through convention plugins in an included `build-logic` build (or `buildSrc`), applied by id. `allprojects {}`/`subprojects {}` blocks couple projects and defeat the configuration cache.
- `settings.gradle.kts` holds `rootProject.name`, `include(":app")` and `dependencyResolutionManagement`. Catalog accessors (`libs.kotlinx.coroutines.core`) are generated and type-checked.
- `./gradlew test` is skipped as `UP-TO-DATE` when its inputs are unchanged; `--rerun` forces it.
- Libraries: `kotlin { explicitApi() }` requires visibility modifiers and return types on every public declaration.
- Annotation processing: KSP where the processor supports it; kapt is in maintenance mode and slower.
- Maven Kotlin builds use `kotlin-maven-plugin` with `<extensions>true</extensions>`; in mixed Java/Kotlin modules the Kotlin compilation must run before `maven-compiler-plugin`.

## Null Safety and Errors

```kotlin
fun displayName(user: User?): String {
    val name = user?.profile?.nickname ?: user?.fullName ?: return "anonymous"
    return name.trim()
}

class Session(private val store: TokenStore) {
    private var token: String? = null

    fun header(): String {
        val current = token ?: store.load().also { token = it }   // a local val smart-casts; the var does not
        return "Bearer $current"
    }
}
```

- Smart casts need a stable value. They fail on `var` properties, `open` properties, properties with custom getters and properties declared in another module. Copy into a local `val` and branch on it.
- Platform types: a value from Java (`String!`) is unchecked, and Kotlin trusts whatever type you declare. State the type at the boundary (`val name: String? = javaUser.getName()`) and annotate Java code you own with JSpecify.
- `!!` throws a bare `NullPointerException`. `requireNotNull(x) { "config key $key missing" }` and `checkNotNull` fail with a message.
- `require(cond) { msg }` throws `IllegalArgumentException`, `check(cond) { msg }` and `error(msg)` throw `IllegalStateException`.
- `lateinit var` is for fields a framework or test setup assigns; reading it early throws `UninitializedPropertyAccessException`. It does not apply to primitive or nullable types.
- `List<String?>` (nullable elements) and `List<String>?` (nullable list) differ. `filterNotNull()`, `mapNotNull {}` and `firstOrNull {}` keep null handling in the types.
- Kotlin has no checked exceptions. Java callers see none unless the function carries `@Throws(IOException::class)`.
- Model expected domain failures as sealed result types so callers must handle each case; throw for bugs and infrastructure faults.
- `runCatching {}` catches every `Throwable`, `CancellationException` and `OutOfMemoryError` included. In coroutine code it breaks cancellation (see Anti-Patterns).
- `use {}` closes a `Closeable`/`AutoCloseable` on every path.

## Data, Sealed and Value Classes

```kotlin
sealed interface PaymentResult {
    data class Approved(val id: String, val amountCents: Long) : PaymentResult
    data class Declined(val reason: String) : PaymentResult
    data object Pending : PaymentResult
}

fun describe(result: PaymentResult): String = when (result) {    // exhaustive: no else branch
    is PaymentResult.Approved if result.amountCents == 0L -> "approved, zero amount"
    is PaymentResult.Approved -> "approved ${result.id}"
    is PaymentResult.Declined -> "declined: ${result.reason}"
    PaymentResult.Pending -> "pending"
}

@JvmInline
value class Email(val value: String) {
    init { require('@' in value) { "invalid email: $value" } }
}
```

- `data class` derives `equals`, `hashCode`, `toString`, `copy` and `componentN` from the primary constructor only; properties declared in the body take no part. `copy` is shallow and runs `init` blocks.
- Keep `var` out of data classes used as map keys or set elements: a mutation changes the hash code and strands the entry.
- A `when` over a sealed type or enum needs no `else`, so a new subtype breaks compilation wherever it must be handled. Guard conditions (`is X if ...`) are stable since Kotlin 2.2.
- `data object` (1.9) gives singleton variants a readable `toString`. `Enum.entries` (1.9) replaces `values()`, which allocates a new array per call.
- Value classes erase to their field at runtime but box when used as a nullable type, a generic type argument or through an interface. Functions taking them get mangled JVM names, so Java callers need `@JvmName` on those functions.
- Destructuring is positional: `val (id, amount) = approved` changes meaning silently when constructor parameters are reordered.

## Coroutines and Structured Concurrency

```kotlin
suspend fun loadDashboard(userId: UserId): Dashboard = coroutineScope {
    val profile = async { profileClient.fetch(userId) }
    val orders = async { orderClient.recent(userId) }
    Dashboard(profile.await(), orders.await())   // a failure in either cancels the other, then rethrows
}

class ReportReader(private val io: CoroutineDispatcher = Dispatchers.IO) {   // injectable for tests
    suspend fun read(path: Path): String = withContext(io) {
        Files.readString(path)                   // blocking call moved off the caller's dispatcher
    }
}
```

- Every coroutine runs in a scope that waits for its children. `coroutineScope {}` fails fast: a child failure cancels the siblings and the scope rethrows. `supervisorScope {}` isolates child failures, and each child handles its own errors.
- `GlobalScope` (`@DelicateCoroutinesApi`) escapes that structure: nothing cancels or awaits its work. Launch background work from a scope you own, such as `CoroutineScope(SupervisorJob() + dispatcher)` cancelled on shutdown, or the framework's scope.
- `launch` when no result is needed, `async` when one is. In a regular scope a failed `async` cancels the parent at once, before anyone calls `await()`.
- Dispatchers: `Default` for CPU work (sized to the cores), `IO` for blocking calls (at least 64 threads), `Dispatchers.IO.limitedParallelism(n)` for a bounded pool per resource. Calls that already suspend (Ktor client, R2DBC) need no `withContext(Dispatchers.IO)`.
- Cancellation is cooperative. kotlinx suspend functions check it; a CPU loop must call `ensureActive()` or `yield()`; blocking Java calls ignore it unless wrapped in `runInterruptible {}`, which turns cancellation into thread interruption.
- `withTimeout` throws `TimeoutCancellationException`, which is a `CancellationException`: inside `launch` it ends the coroutine quietly, with nothing logged. `withTimeoutOrNull` returns `null` instead.
- `runBlocking` belongs at the top of `main` and in bridges from blocking code. Inside a coroutine it blocks the dispatcher thread.
- The compiler rejects a suspension point inside `synchronized {}`. Use `kotlinx.coroutines.sync.Mutex` (not reentrant) or confine the state to one coroutine.

## Flow

```kotlin
fun prices(symbol: String): Flow<Price> = flow {
    while (true) {
        emit(api.quote(symbol))
        delay(1_000)
    }
}
    .flowOn(Dispatchers.IO)                              // upstream on IO; the collector keeps its context
    .catch { e -> log.warn("quote stream failed", e) }   // catches upstream failures only

val latest: StateFlow<Price?> = prices("ACME")
    .stateIn(scope, SharingStarted.WhileSubscribed(5_000), initialValue = null)
```

- `flow {}` is cold: the block runs once per collector. `shareIn`/`stateIn` share one upstream among many collectors.
- `flowOn` changes the context of the operators above it. Calling `withContext` inside `flow {}` and emitting from there throws `IllegalStateException` (flow invariant violated).
- `catch` sees upstream exceptions only; a failure inside `collect {}` passes it by.
- `StateFlow` is conflated and skips equal values: assigning the current value emits nothing, and a slow collector sees only the latest. Change it with `state.update { it.copy(...) }` (atomic compare-and-set); `state.value = state.value.copy(...)` loses concurrent updates.
- `collectLatest` cancels the previous block when a new value arrives.

## Ktor

```kotlin
fun Application.module() {
    install(ContentNegotiation) { json() }
    install(StatusPages) {
        exception<IllegalArgumentException> { call, cause ->
            call.respond(HttpStatusCode.BadRequest, mapOf("error" to (cause.message ?: "bad request")))
        }
    }
    routing {
        get("/users/{id}") {
            val id = call.parameters["id"]?.toLongOrNull()
                ?: return@get call.respond(HttpStatusCode.BadRequest)
            val user = userService.find(id) ?: return@get call.respond(HttpStatusCode.NotFound)
            call.respond(user)                           // User is @Serializable
        }
    }
}

fun main() {
    embeddedServer(Netty, port = 8080, module = Application::module).start(wait = true)
}
```

- Route handlers are suspend functions on the engine's threads. Wrap blocking calls (JDBC, file IO) in `withContext(Dispatchers.IO)`.
- Create one `HttpClient` per application and close it on shutdown; each instance owns an engine and a connection pool. Set `expectSuccess = true` to turn non-2xx responses into `ResponseException`; by default any status returns normally.
- Keep routes thin: parse and validate in the handler, call a service, map domain results to status codes in one place (`StatusPages`).

## Spring with Kotlin

```kotlin
plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.spring)   // opens classes annotated @Component, @Transactional, @Async, @Cacheable, @SpringBootTest
    alias(libs.plugins.kotlin.jpa)      // no-arg constructors for @Entity, @Embeddable, @MappedSuperclass
    alias(libs.plugins.spring.boot)
}

allOpen {                                // lazy-loading proxies subclass entities
    annotation("jakarta.persistence.Entity")
    annotation("jakarta.persistence.MappedSuperclass")
    annotation("jakarta.persistence.Embeddable")
}
```

```kotlin
@ConfigurationProperties("spool")
data class SpoolProperties(val directory: String, val maxFiles: Int = 100)

@Service
class SpoolService(private val repository: SpoolRepository) {   // primary constructor injection
    @Transactional
    fun archive(id: Long) {
        val entry = repository.findByIdOrNull(id) ?: throw EntryNotFoundException(id)
        entry.archived = true
    }
}
```

- Kotlin classes and methods are final. Without `plugin.spring`, `@Configuration` classes fail at startup and `@Transactional` on a final method is skipped silently, since the proxy cannot override it.
- Entities are regular classes with `var` properties. A `data class` entity derives `equals`/`hashCode`/`toString` from mutable fields and lazy associations.
- `jackson-module-kotlin` on the classpath (Boot registers it) lets Jackson construct classes through primary constructors and honour default values.
- `-Xjsr305=strict` turns Spring's nullability annotations into Kotlin nullable and non-null types.
- WebFlux controllers may be `suspend` functions and may return `Flow`.
- Self-invocation, rollback rules and `open-in-view` behave as in Java Spring (see `loom-java`).

## Testing

```kotlin
class UploaderTest {
    private val client = mockk<UploadClient>()

    @Test
    fun retriesOnceAfterTransientFailure() = runTest {
        coEvery { client.upload(any()) } throws IOException("reset") andThen Receipt("r1")

        val receipt = Uploader(client, retryDelay = 5.seconds).upload(Payload("x"))

        assertEquals(Receipt("r1"), receipt)             // the 5 s retry delay ran on virtual time
        coVerify(exactly = 2) { client.upload(any()) }
    }

    @Test
    fun `rejects an empty payload`() {
        assertFailsWith<IllegalArgumentException> { Payload("") }
    }
}
```

- `kotlin("test")` with `useJUnitPlatform()` runs `kotlin.test` tests on JUnit 5, and JUnit 5 features (`@TempDir`, `@ParameterizedTest`) work alongside. Backtick names read well in reports; the JVM method name then contains the spaces.
- `runTest` runs on virtual time: `delay` returns at once and the test clock advances. Code that switches to a real dispatcher leaves virtual time, so inject dispatchers and pass `StandardTestDispatcher(testScheduler)` in tests. `runBlocking` in a test waits in real time.
- MockK: `every`/`coEvery` stub, `verify`/`coVerify` check. `relaxed = true` returns defaults for unstubbed calls and hides a missing stub. `mockkObject`/`mockkStatic` patch global state; call `unmockkAll()` in `@AfterEach`.
- kotest: spec styles (`FunSpec`, `StringSpec`, `BehaviorSpec`), matchers (`shouldBe`, `shouldThrow<T> {}`), property tests (`checkAll`). Specs run on the JUnit Platform through `kotest-runner-junit5`; the matcher and property modules also work inside plain JUnit tests.
- Flows: Turbine (`flow.test { assertEquals(first, awaitItem()); awaitComplete() }`).
- Ktor: `testApplication { application { module() }; assertEquals(HttpStatusCode.OK, client.get("/health").status) }` runs the module in memory.

## Loom Test Runner Adapter

**Adapter.** A Kotlin project built with Gradle uses `gradle`; a Maven Kotlin build uses `maven`. `loom project detect` prints the choice per package. `build.gradle.kts` or `settings.gradle.kts` make a directory kind `kotlin`; `pom.xml`, `build.gradle` or `settings.gradle` make it kind `java`. For both kinds the runner is `gradle` when a `build.gradle*` exists, else `maven`. So a Kotlin project built by Maven or by a Groovy-DSL `build.gradle` reports kind `java` with the `loom-java` skill, and a Java project with a `build.gradle.kts` reports kind `kotlin`; the runner is right in each case. A contract's optional `runner:` field (`runner: maven`, `runner: gradle`) names the adapter when detection picks the wrong one.

```text
app  kinds=kotlin  runner=gradle  skills=loom-kotlin
```

**Single-test command.** loom runs it with the package directory as the working directory:

```bash
# gradle, when gradlew exists in the package directory
./gradlew test --tests '{test}'
# gradle, without a wrapper there
gradle test --tests '{test}'
# maven
mvn -q test -Dtest='{test}'
```

The commands are fixed. Subprojects of a multi-project build hold no `gradlew`, so loom runs `gradle` from PATH there, and the `maven` adapter always runs `mvn` from PATH; install a matching tool on the host when either applies. Android and Kotlin Multiplatform modules run unit tests through variant or target tasks (`testDebugUnitTest`, `jvmTest`), while the command targets the plain JVM `test` task: keep contract tests in a JVM module.

**The `test` field.**

| Adapter | `test` value | Example |
| --- | --- | --- |
| `gradle` | `package.Class.method` | `com.acme.upload.UploaderTest.retriesOnceAfterTransientFailure` |
| `maven` | `Class#method` | `UploaderTest#retriesOnceAfterTransientFailure` |

`Class` is the class holding the test function; top-level functions are never tests. A backtick-named test keeps its spaces in the method part (`com.acme.upload.UploaderTest.rejects an empty payload`), and the command quotes it, but the value must match character for character.

**No-match behaviour.** D5 marks `gradle` and `maven` "documented": no fixture was captured, and loom's parsers for them were written from the runners' documented output. Gradle fails the task with `No tests found for given includes`; Surefire fails with `No tests matching pattern ... were executed`. Both exit non-zero, like a failing test, so the exit code cannot tell a misspelled name from a red test. loom reads the runner's summary instead: a name that selects nothing classifies as `NotSelected`, which fails the freeze ("the runner did not select the test") and fails the completion check ("contract test not selected").

**Writing contract tests.** Test sources live under `src/test/kotlin/` in the directory of their package, for example `src/test/kotlin/com/acme/upload/UploaderTest.kt`. For the adapter to select exactly one test:

- one contract per `@Test` function in a top-level class; `@Nested` inner classes change the class name the filter sees (`Outer$Inner`);
- a camelCase function name, unique in its class;
- kotest specs are selected by spec class, and their tests are strings rather than methods, so write each contract as a `@Test` function in a plain class. It runs next to the kotest specs on the JUnit Platform once `kotlin("test")` or `junit-jupiter` is on the test classpath.

```yaml
contracts:
  - id: retries-once-after-transient-failure
    file: src/test/kotlin/com/acme/upload/UploaderTest.kt
    test: com.acme.upload.UploaderTest.retriesOnceAfterTransientFailure
    scenario: the upload client throws IOException("reset") once, then returns a receipt
    rejects: an uploader that returns the first IOException to the caller without retrying
```

In a Maven package the same contract reads `test: UploaderTest#retriesOnceAfterTransientFailure`.

**Build failures.** A contract test that does not compile yet counts as red at freeze time: loom classifies the compiler failure as `BuildFailed`. `compileTestKotlin` compiles the whole test source set, so one uncompilable contract file makes every test in that module `BuildFailed` until the implementation exists. Freeze and completion run each command under a 300-second limit: run the single-test command once before freezing so the Gradle daemon start and dependency download happen outside the timed run. A contract that needs a new test dependency edits a build file, which must match one of the stage's `harness` globs.

## Anti-Patterns

```kotlin
// !! chains: the NPE names no value
val city = user!!.address!!.city!!                                   // BAD
val city = user?.address?.city ?: error("user $id has no city")      // GOOD

// runCatching in a coroutine swallows cancellation
val result = runCatching { client.fetch(id) }                        // BAD
val result = try {                                                   // GOOD
    Result.success(client.fetch(id))
} catch (e: CancellationException) {
    throw e
} catch (e: IOException) {
    Result.failure(e)
}

// Fire-and-forget outside structured concurrency
GlobalScope.launch { audit(event) }        // BAD: nothing cancels or awaits it
appScope.launch { audit(event) }           // GOOD: scope cancelled on shutdown

// Blocking call on the caller's dispatcher
suspend fun load() = Files.readString(path)                                  // BAD
suspend fun load() = withContext(Dispatchers.IO) { Files.readString(path) }  // GOOD
```

Quick swaps: `if (x != null) x.foo() else null` → `x?.foo()`; `filter { }.first()` → `first { }`; `map { }.filterNotNull()` → `mapNotNull { }`; `for (i in 0 until list.size)` → `for (item in list)` or `withIndex()`; `lateinit var` for a value known at construction → a constructor `val`; Java-style `getX()`/`setX()` → properties; mutable state in a `companion object` → an injected dependency; `MutableList` in a public signature → a private `_items` backing property exposed as `List`; `let`/`apply`/`also` nested three deep → named locals.

## Expert Practices

### Collections and Sequences

- `List` and `Map` are read-only interfaces, which is weaker than immutable: the object may be a `MutableList` that its owner still changes. Copy with `toList()` at trust boundaries.
- Collection operators are eager and allocate a list per step. For long chains over large inputs use `asSequence()` (lazy, one pass); for small collections the eager form is faster.
- `associateBy` keeps the last element per key without warning; `groupBy` keeps them all. `single()` throws unless exactly one element matches.

### Scope Functions

- `let` (transform a nullable, `it`), `apply` (configure the receiver, returns it), `also` (side effect, returns the value), `run` (compute with a receiver). Nesting them makes `this` and `it` ambiguous; past one level, name a local.

### Java Interop

- Default arguments are invisible to Java callers without `@JvmOverloads`. `@JvmStatic` exposes companion members as static methods, `@JvmField` exposes a field without accessors, `@JvmName` resolves signature clashes, `@Throws` declares checked exceptions for Java.
- An `object` is `Foo.INSTANCE` from Java. `fun interface` gives Kotlin interfaces SAM conversion.

### Security and Serialization

- kotlinx.serialization: annotate classes `@Serializable`. The default `Json` rejects unknown keys; `Json { ignoreUnknownKeys = true }` tolerates evolving input. Polymorphic decoding works through registered sealed hierarchies, so input never names a class to instantiate.
- String templates make injection easy to write: `"select * from users where name = '$name'"` and `ProcessBuilder("sh", "-c", "tar $file")` are both injectable. Bind parameters (Exposed's DSL, JDBC `PreparedStatement`) and pass argument lists.
- The Java rules hold unchanged: no `ObjectInputStream` on untrusted bytes, XML parsers with DTDs disabled, `SecureRandom` for tokens, normalized paths checked against their base.

### Performance

- `inline` functions with lambda parameters avoid allocating the lambda; `reified` type parameters require `inline`. Inlining a large function copies its body into every call site.
- `lazy {}` synchronizes by default; `lazy(LazyThreadSafetyMode.NONE)` skips the lock for single-threaded use.
- Coroutines are cheap, but one `launch` per item over a million items still allocates a million coroutines. Bound the fan-out with a `Semaphore`, `limitedParallelism`, or `flatMapMerge(concurrency = n)`.
- Top-level `val` initializers run when the file's facade class (`FooKt`) loads. Use `const val` for compile-time constants and `lazy` for expensive values.

## Verification Checklists

**Before marking Kotlin work done:**

- [ ] `./gradlew check` passes with `allWarningsAsErrors`; detekt and ktlint (or ktfmt) clean
- [ ] New behaviour has tests; the test report shows a non-zero count for the new classes
- [ ] No `!!` on values that can be absent; Java platform types given explicit Kotlin types at the boundary
- [ ] Public API exposes read-only collection types; no `var` in data classes used as keys
- [ ] `when` over sealed types and enums has no `else` branch
- [ ] No string templates in SQL, shell commands or log patterns built from untrusted input

**Coroutines and framework review:**

- [ ] No `GlobalScope`; every launched coroutine has an owning scope that is cancelled
- [ ] `CancellationException` is rethrown; no `runCatching` around suspend calls
- [ ] Blocking calls run under `withContext(Dispatchers.IO)` or an injected dispatcher; no `runBlocking` inside coroutines
- [ ] `StateFlow` changes use `update {}`; flows switch context with `flowOn`
- [ ] Spring: `plugin.spring` and `plugin.jpa` applied; entities are plain classes; `-Xjsr305=strict` set
- [ ] Coroutine tests use `runTest` with injected test dispatchers; mocks of global state are undone after each test
