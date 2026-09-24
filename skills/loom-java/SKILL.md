---
name: loom-java
description: Java language expertise for idiomatic, production-quality code.
triggers:
  - java
  - jdk
  - jvm
  - javac
  - maven
  - mvn
  - pom.xml
  - surefire
  - gradle
  - gradlew
  - junit
  - jupiter
  - assertj
  - mockito
  - testcontainers
  - spring
  - spring boot
  - jpa
  - hibernate
  - jakarta
  - record
  - sealed
  - virtual threads
  - completablefuture
  - optional
---

# Java Language Expertise

## Overview

Idiomatic, production-grade Java 21+ (25 is the current LTS): records, sealed hierarchies, pattern matching, streams, virtual threads, the Maven and Gradle builds, JUnit 5 and Spring Boot. Assumes the reader knows the language. The content is decision rules and the traps that survive review: `==` on boxed values, `Collectors.toMap` on a duplicate key, `@Transactional` on a self-call, a pinned virtual thread, a test task that runs zero JUnit 5 tests and reports success.

## Tooling

Use the wrapper the repository ships (`./gradlew`, `./mvnw`); it pins the build tool version. Keep the existing build tool: a Maven-to-Gradle migration is a separate, requested change.

| Task | Gradle | Maven |
| --- | --- | --- |
| Full gate | `./gradlew check` | `./mvnw -q verify` |
| Compile main and tests | `./gradlew testClasses` | `./mvnw -q test-compile` |
| Unit tests | `./gradlew test` | `./mvnw -q test` |
| One test | `./gradlew test --tests 'com.acme.FooTest.bar'` | `./mvnw test -Dtest='FooTest#bar'` |
| Dependency tree | `./gradlew dependencies --configuration runtimeClasspath` | `./mvnw dependency:tree` |
| Why is X on the classpath | `./gradlew dependencyInsight --dependency jackson-databind` | `./mvnw dependency:tree -Dincludes=com.fasterxml.jackson.core` |
| Format | `./gradlew spotlessApply` | `./mvnw spotless:apply` |

- **Gradle:** `check` runs `test` plus every verification task a plugin attaches (Spotless's `spotlessCheck`, Checkstyle, SpotBugs); Error Prone runs inside `compileJava`. `build` is `check` plus `assemble`. A `test` task whose inputs have not changed is skipped as `UP-TO-DATE`; `--rerun` (Gradle 7.6+) forces it.
- **Maven:** Surefire runs unit tests in the `test` phase (default includes `**/Test*.java`, `**/*Test.java`, `**/*Tests.java`, `**/*TestCase.java`); Failsafe runs `*IT.java` in `integration-test`/`verify`. Pin `maven-surefire-plugin` 3.x in `<pluginManagement>`: older Maven super-POMs bind Surefire 2.12.4, which runs no JUnit 5 tests and reports success.
- **Versions:** Gradle keeps them in the version catalog (`gradle/libs.versions.toml`) and imports BOMs with `platform(...)`; Maven imports BOMs in `<dependencyManagement>` with `<scope>import</scope>`. Conflict resolution differs: Gradle picks the highest requested version, Maven picks the nearest declaration in the tree, which can silently downgrade a transitive dependency. `maven-enforcer-plugin`'s `dependencyConvergence` rule turns that into a build error.
- **Static analysis:** `javac -Xlint:all -Werror`; Error Prone (javac plugin, compile-time bug patterns); NullAway (an Error Prone check) with JSpecify `@NullMarked`/`@Nullable`; SpotBugs on bytecode; Checkstyle or PMD for style rules. Format with Spotless running google-java-format or palantir-java-format.

```kotlin
// build.gradle.kts for a Java project
plugins {
    java
    alias(libs.plugins.spotless)
    alias(libs.plugins.errorprone)
}

java {
    toolchain { languageVersion = JavaLanguageVersion.of(21) }
}

dependencies {
    testImplementation(platform(libs.junit.bom))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    testImplementation(libs.assertj.core)
    errorprone(libs.errorprone.core)
}

tasks.withType<JavaCompile>().configureEach {
    options.compilerArgs.addAll(listOf("-Xlint:all", "-Werror"))
}

tasks.test {
    useJUnitPlatform()   // without it Gradle runs the JUnit 4 engine, which does not see Jupiter tests
}
```

## Records, Sealed Types and Pattern Matching

Model data as records and closed sets of alternatives as sealed interfaces; `switch` then checks exhaustiveness (Java 21).

```java
public sealed interface Command {
    record Upload(Path source, String target) implements Command {
        public Upload {                                   // compact constructor: runs on every construction
            Objects.requireNonNull(source, "source");
            if (target == null || target.isBlank()) throw new IllegalArgumentException("target is blank");
        }
    }
    record Delete(String target) implements Command {}
    record Batch(List<Command> commands) implements Command {
        public Batch { commands = List.copyOf(commands); }   // defensive copy; rejects null elements
    }
}

static String describe(Command command) {
    return switch (command) {                             // exhaustive over the sealed type: no default
        case Command.Upload(Path source, var target) -> "upload " + source + " -> " + target;
        case Command.Delete(String target) -> "delete " + target;
        case Command.Batch(var commands) when commands.isEmpty() -> "empty batch";
        case Command.Batch(var commands) -> "batch of " + commands.size();
    };
}
```

- Records nested in the sealed interface need no `permits` clause. Jackson (2.12+) deserializes records through the canonical constructor, so compact-constructor validation also guards parsed input.
- Records are shallowly immutable. Copy mutable components (`List.copyOf`, `Map.copyOf`) in the compact constructor. An array component keeps reference equality: `record Blob(byte[] data)` has an `equals` that compares array identity; override `equals`/`hashCode` with `Arrays.equals`/`Arrays.hashCode` or hold an immutable type.
- Leave `default` out of a `switch` over a sealed type. A new subtype then fails compilation at every switch that must handle it; a `default` branch absorbs it silently.
- `instanceof` patterns bind and narrow in one step: `if (obj instanceof String s && !s.isBlank())`. The unnamed pattern `_` (Java 22) ignores record components you do not use.
- Enums for fixed sets without per-instance data; `EnumMap`/`EnumSet` are array-backed and faster than hash collections keyed by enums.
- `var` for locals whose type the right-hand side states (`var users = new ArrayList<User>()`); spell the type when the initializer is a call whose return type the reader cannot see.

## Errors and Null Handling

```java
public Config load(Path path) throws ConfigException {
    try (var reader = Files.newBufferedReader(path)) {          // closed on every exit path
        return parse(reader);
    } catch (NoSuchFileException e) {                            // specific before general
        throw new ConfigException("config not found: " + path, e);
    } catch (IOException e) {
        throw new ConfigException("reading config " + path, e);  // always pass the cause
    }
}
```

- Checked exceptions for conditions a caller can act on (missing file, refused connection); unchecked (`IllegalArgumentException`, `IllegalStateException`) for bugs and violated preconditions. Checked exceptions do not pass through lambdas: wrap in `UncheckedIOException` inside a stream and unwrap at the boundary.
- Pass the cause to every wrapping exception. `throw new X(msg)` inside a `catch` discards the original stack trace.
- try-with-resources closes resources in reverse order; an exception thrown by `close()` is attached to the primary one as suppressed (`getSuppressed()`).
- `InterruptedException`: restore the flag with `Thread.currentThread().interrupt()` before returning or rethrowing unchecked. Swallowing it breaks cancellation for every caller up the stack.
- `Optional` is a return type. Keep it out of fields, parameters and collections. `orElse(compute())` always evaluates `compute()`; `orElseGet(this::compute)` is lazy. Prefer `orElseThrow()` to `get()`.
- Null: `Objects.requireNonNull(x, "x")` at public boundaries; JSpecify `@NullMarked` on the package plus `@Nullable` on the exceptions, checked at compile time by NullAway. Return empty collections instead of `null`.

## Streams and Collections

```java
Map<String, Integer> totals = orders.stream()
    .collect(Collectors.toMap(Order::customer, Order::amount, Integer::sum)); // merge: duplicates add up

List<String> names = users.stream()
    .filter(User::active)
    .map(User::name)
    .toList();                                   // unmodifiable (Java 16+)

Map<Status, List<Order>> byStatus = orders.stream()
    .collect(Collectors.groupingBy(Order::status, () -> new EnumMap<>(Status.class), Collectors.toList()));
```

- `Collectors.toMap` without a merge function throws `IllegalStateException` on a duplicate key, and `NullPointerException` on a null value.
- `Stream.toList()` returns an unmodifiable list that allows nulls; `Collectors.toList()` makes no promise about mutability; `Collectors.toUnmodifiableList()` rejects nulls.
- `List.of`, `Set.of`, `Map.of` are immutable and reject nulls; `Set.of`/`Map.of` throw on duplicates at construction, and their iteration order is unspecified and changes between JVM runs. Sort, or use `LinkedHashMap`/`TreeMap`, wherever order is observable (JSON output, snapshots, log lines under test).
- `Arrays.asList` is fixed-size and writes through to the backing array.
- Sequenced collections (21): `getFirst()`, `getLast()`, `reversed()` on lists, deques, `LinkedHashSet` and `LinkedHashMap`.
- Streams are lazy and single-use. Side effects in `peek` or `map` may never run: since Java 9, `count()` skips the pipeline when the size is known from the source.
- `parallelStream()` runs on the JVM-wide common `ForkJoinPool`. Blocking IO inside it starves every other parallel stream and every `CompletableFuture.supplyAsync` without an executor. Use it only for CPU-bound work on large inputs, measured.
- Stream gatherers (final in 24): `stream.gather(Gatherers.windowFixed(3))`, `windowSliding`, and `Gatherers.mapConcurrent(n, fn)` for bounded concurrent mapping on virtual threads.

## Concurrency

```java
try (ExecutorService executor = Executors.newVirtualThreadPerTaskExecutor()) {
    List<Future<String>> pages = urls.stream()
        .map(url -> executor.submit(() -> fetch(url)))      // one virtual thread per blocking call
        .toList();
    for (Future<String> page : pages) {
        consume(page.get());                                // ExecutionException wraps the task's failure
    }
}                                                           // close() waits for every submitted task
```

- **Virtual threads (21)** are for blocking IO at high concurrency. Create one per task and never pool them. The executor has no size limit, so bound calls to a downstream service with a `Semaphore`.
- **Pinning:** before JDK 24, blocking inside `synchronized` pinned the carrier thread; JEP 491 (JDK 24) removed that for `synchronized`. Native frames (JNI, some drivers) still pin. On 21, guard blocking calls with `ReentrantLock` and watch the `jdk.VirtualThreadPinned` JFR event.
- `ThreadLocal` works on virtual threads, but each thread gets its own copy: a per-thread cache of heavy objects multiplies by the thread count. `ScopedValue` (final in 25) passes request context down a call tree immutably.
- CPU-bound work belongs on a platform pool sized to the cores (`Executors.newFixedThreadPool(Runtime.getRuntime().availableProcessors())`) or a `ForkJoinPool`.
- `CompletableFuture.supplyAsync(fn)` without an executor runs on the common pool; pass one. `join()` throws unchecked `CompletionException`, `get()` throws checked `ExecutionException`. Attach `exceptionally`/`handle` at the end of a chain; a failed future nobody observes fails silently.
- `StructuredTaskScope` is still a preview API in 25 and its shape changed between releases; keep it out of code that must build without `--enable-preview`.
- Shared state: `ConcurrentHashMap.merge`/`compute` for atomic read-modify-write (a `get` followed by `put` races); `LongAdder` for contended counters, `AtomicLong` otherwise; `volatile` for a flag with one writer. Immutable records cross threads without locks.
- Spring Boot 3.2+: `spring.threads.virtual.enabled=true` serves requests on virtual threads.

## Packages and Modules

- Standard layout: `src/main/java`, `src/main/resources`, `src/test/java`, `src/test/resources`; the directory path matches the package.
- Package-private (no modifier) is the default for implementation classes; mark `public` only the API. Tests in the same package reach package-private members without widening them.
- JPMS (`module-info.java`): `exports` the API packages, `requires transitive` for types your API exposes, `opens` for packages that reflective frameworks (Jackson, Hibernate, Spring) must read. A missing `opens` surfaces at runtime as `InaccessibleObjectException`. Applications that publish no library usually stay on the classpath.

## Spring Boot Essentials

```java
@ConfigurationProperties(prefix = "spool")
@Validated
public record SpoolProperties(@NotBlank String directory, @Positive int maxFiles) {}

@Service
class SpoolService {
    private final SpoolRepository repository;

    SpoolService(SpoolRepository repository) {      // single constructor: injected without @Autowired
        this.repository = repository;
    }

    @Transactional
    public void archive(long id) {
        var entry = repository.findById(id).orElseThrow(() -> new EntryNotFoundException(id));
        entry.markArchived();                        // dirty checking writes the change at commit
    }
}
```

- Register properties with `@ConfigurationPropertiesScan` or `@EnableConfigurationProperties(SpoolProperties.class)`. Records bind through the constructor; `@Validated` fails startup on invalid values.
- Proxy-based annotations (`@Transactional`, `@Async`, `@Cacheable`) do nothing on a self-call: `this.archive(id)` from the same bean bypasses the proxy. Move the method to another bean.
- Rollback happens by default only for unchecked exceptions and `Error`; a checked exception commits. Use `@Transactional(rollbackFor = Exception.class)` where checked exceptions signal failure.
- Set `spring.jpa.open-in-view=false`. The default keeps the persistence context open through response rendering, hides lazy-loading N+1 queries and holds a connection for the whole request (Boot logs a warning about it). Load what a use case needs with `JOIN FETCH` or `@EntityGraph`.
- JPA entities are classes with a no-arg constructor. Records cannot be entities. Base `equals`/`hashCode` on the id or a business key, and keep Lombok `@Data` off entities: its `toString`/`hashCode` walk lazy associations.
- Actuator: expose `health` and `info`; keep `env`, `heapdump`, `loggers` and the rest behind authentication.
- Boot 3+ uses the `jakarta.*` namespace (`jakarta.persistence`, `jakarta.validation`). `javax.*` imports mark Boot 2 era code.

## Testing

```java
class SpoolWriterTest {
    @TempDir Path root;

    @Test
    void writesEntryUnderRoot() throws IOException {
        new SpoolWriter(root).write("a.json", "{}");
        assertThat(root.resolve("a.json")).hasContent("{}");
    }

    @ParameterizedTest
    @ValueSource(strings = {"../escape.json", "/etc/passwd", "a/../../b"})
    void rejectsPathsOutsideRoot(String name) {
        var writer = new SpoolWriter(root);
        assertThatThrownBy(() -> writer.write(name, "{}"))
            .isInstanceOf(IllegalArgumentException.class)
            .hasMessageContaining("outside spool root");
    }
}
```

- JUnit Jupiter (JUnit 5; JUnit 6 keeps the `org.junit.jupiter.api` annotations): `@Test`, `@ParameterizedTest` with `@ValueSource`/`@CsvSource`/`@MethodSource`, `@TempDir`, `@Nested`, `@BeforeEach`. Test classes and methods can be package-private.
- AssertJ (`assertThat(...)`, `assertThatThrownBy`) produces better failure messages than `assertEquals`. JUnit's `assertThrows` returns the exception for further checks.
- Mockito with `@ExtendWith(MockitoExtension.class)` uses strict stubs: an unused stub fails the test. Mock roles you own; build value objects for real.
- Integration tests: Testcontainers with the production database engine. H2 in a compatibility mode differs in SQL dialect and locking.
- Spring slices (`@WebMvcTest`, `@DataJpaTest`, `@JsonTest`) load a narrow context; `@SpringBootTest` loads all of it. Each distinct context configuration (another set of mocked beans, other properties) starts a new context, the usual cause of slow Spring suites. `@MockitoBean` (Spring Framework 6.2) replaces the deprecated `@MockBean`.
- Asynchronous outcomes: Awaitility (`await().atMost(Duration.ofSeconds(5)).until(...)`) instead of `Thread.sleep`.
- Zero-test traps: Gradle without `useJUnitPlatform()` and Maven with Surefire older than 2.22 both run no Jupiter tests. Check the report's test count after changing the build.

## Loom Test Runner Adapter

**Adapter.** Java uses `gradle` or `maven`. `loom project detect` prints the choice per package. A directory with `pom.xml`, `build.gradle` or `settings.gradle` is kind `java` (`build.gradle.kts` or `settings.gradle.kts` make it kind `kotlin`, which follows the same rule), and the runner is `gradle` when a `build.gradle*` exists there, else `maven`. Bazel and Ant builds carry none of these markers and get no Java adapter. A contract's optional `runner:` field names the adapter when detection picks the wrong one.

```text
service  kinds=java  runner=maven  skills=loom-java
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

The commands are fixed. The `maven` adapter runs `mvn` from PATH even when the project ships `./mvnw`, and the subprojects of a Gradle multi-project build hold no `gradlew`, so loom runs `gradle` from PATH there. Install a matching Maven or Gradle on the host when either applies. When rerunning a command by hand, drop `-q`: quiet mode hides Surefire's `Tests run:` line on a passing run.

**The `test` field.**

| Adapter | `test` value | Example |
| --- | --- | --- |
| `gradle` | `package.Class.method` | `com.acme.spool.SpoolWriterTest.rejectsSymlinkedSpool` |
| `maven` | `Class#method` | `SpoolWriterTest#rejectsSymlinkedSpool` |

Both runners filter on the Java class and method names; `@DisplayName` plays no part in selection. Maven's `Class#method` uses the simple class name, so two `SpoolWriterTest` classes in different packages both run: keep contract test class names unique within the module.

**No-match behaviour.** D5 marks both runners "documented": no fixture was captured, and loom's `gradle` and `maven` parsers were written from the runners' documented output. Gradle fails the task with `No tests found for given includes`; Surefire fails with `No tests matching pattern ... were executed`. Both exit non-zero, like a failing test, so the exit code cannot tell a misspelled name from a red test. loom reads the runner's summary instead: a name that selects nothing classifies as `NotSelected`, which fails the freeze ("the runner did not select the test") and fails the completion check ("contract test not selected").

**Writing contract tests.** Test sources live under `src/test/java/` in the directory of their package, for example `src/test/java/com/acme/spool/SpoolWriterTest.java`. Name classes `*Test` so Surefire's default includes pick them up; `*IT` classes belong to Failsafe, which `mvn test` does not run. For the adapter to select exactly one test:

- one contract per plain `@Test` method in a top-level class; a `@Nested` class changes the name the filter sees (`Outer$Inner`);
- a camelCase method name, unique in its class, with no overloads;
- the `test` value copied from the source, with the package for `gradle`.

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: src/test/java/com/acme/spool/SpoolWriterTest.java
    test: com.acme.spool.SpoolWriterTest.rejectsSymlinkedSpool
    scenario: the spool directory is a symlink to a directory outside the data root
    rejects: a writer that opens the spool path without LinkOption.NOFOLLOW_LINKS and writes through the link
```

In a Maven package the same contract reads `test: SpoolWriterTest#rejectsSymlinkedSpool`.

**Build failures.** A contract test that does not compile yet, because it calls a class or method the stage has not written, counts as red at freeze time: loom classifies the compiler failure as `BuildFailed`. javac compiles the whole test source set together, so one uncompilable contract file makes every test in that module `BuildFailed` until the implementation exists. Freeze and completion run each command under a 300-second limit: run the single-test command once before freezing so the daemon start and dependency download happen outside the timed run. If a contract needs a new test dependency, the build file it edits must match one of the stage's `harness` globs; the freeze accepts only contract files and harness matches.

## Anti-Patterns

```java
// Boxed comparison: == compares references, and Integer caches only -128..127
Integer a = 1000, b = 1000;
boolean same = a == b;                   // BAD: false
boolean equal = a.equals(b);             // GOOD

// Ternary unboxing: the expression type is int, so a null from the map throws NPE
Integer count = found ? cache.get(key) : 0;              // BAD
Integer count = found ? cache.get(key) : Integer.valueOf(0);   // GOOD

// Removing while iterating
for (String s : list) if (s.isBlank()) list.remove(s);   // BAD: ConcurrentModificationException
list.removeIf(String::isBlank);                          // GOOD

// BigDecimal
new BigDecimal(0.1);                                     // BAD: 0.1000000000000000055511151231257827...
new BigDecimal("0.1");                                   // GOOD (or BigDecimal.valueOf(0.1))
new BigDecimal("1.0").equals(new BigDecimal("1.00"));    // false: equals compares scale
new BigDecimal("1.0").compareTo(new BigDecimal("1.00")) == 0;   // true
```

Quick swaps: field `@Autowired` → constructor injection with `final` fields; `Date`/`SimpleDateFormat` (mutable, not thread-safe) → `java.time` (`Instant`, `LocalDate`, `DateTimeFormatter`); `e.printStackTrace()` → a logger call with the exception as the last argument; raw `List` → `List<String>`; empty `catch` → handle, log with context, or rethrow; `return null` for "no results" → an empty collection; `+=` on a `String` in a loop → `StringBuilder` or `Collectors.joining`; `synchronized (this)` in a public class → a private final lock object; `equals` without `hashCode` → both, or a record; `list.remove(1)` on a `List<Integer>` removes index 1 → `list.remove(Integer.valueOf(1))` to remove the value.

## Expert Practices

### Equality and Hashing

- `equals` and `hashCode` must agree. Records generate both over all components; hand-written classes use `Objects.equals` and `Objects.hash` (which allocates a varargs array, so hand-roll it in hot paths).
- A mutable object used as a `HashMap` key or `HashSet` element, then mutated, lands in the wrong bucket: `contains` returns false and the entry leaks. Key maps by immutable values.
- `switch` with a `default` over a sealed type hides new subtypes (see Records above).

### Strings, Locales and Charsets

- `toLowerCase()`/`toUpperCase()` use the default locale: under a Turkish locale `"TITLE".toLowerCase()` yields a dotless `ı`. Pass `Locale.ROOT` for identifiers, protocol keywords, header names and file extensions.
- `String.format` and `formatted` also use the default locale (`%.2f` prints `1,50` under `de_DE`). Use `Locale.ROOT` for machine-readable output.
- The default charset is UTF-8 since JDK 18 (JEP 400); older runtimes used the platform encoding. Pass `StandardCharsets.UTF_8` explicitly in code that still runs on 17.
- `String.split` takes a regex and drops trailing empty strings: `"a.b".split(".")` returns an empty array. Use `split(Pattern.quote("."), -1)` to split on a literal and keep empties.

### Language Gotchas

- `int`/`long` arithmetic wraps silently on overflow. `Math.addExact`, `multiplyExact` and `toIntExact` throw `ArithmeticException` instead; `Math.abs(Integer.MIN_VALUE)` is still negative.
- `Collections.unmodifiableList(list)` is a read-only view: later changes to `list` show through it. `List.copyOf` takes a snapshot.
- `Arrays.asList(new int[] {1, 2})` is a `List<int[]>` with one element. Use `List.of(1, 2)` or `IntStream.of(...).boxed()`.
- Enum and `Object` hash codes are identity-based and differ between runs, so a `HashSet` of enums iterates in a different order each run. `EnumSet`/`EnumMap` iterate in declaration order.
- Static fields initialize in textual order. A static read that runs before the field's initializer (a circular class initialization, or a static method called from an earlier initializer) sees `null` or `0`.
- Money in `double` accumulates rounding error. Use `BigDecimal` with an explicit `RoundingMode`, or integer minor units.

### Security

- Never call `ObjectInputStream.readObject` on untrusted bytes: gadget chains in the classpath turn it into remote code execution. Where it cannot be removed, install an `ObjectInputFilter` allow-list.
- Jackson: `activateDefaultTyping` or `@JsonTypeInfo(use = Id.CLASS)` on untrusted input lets the payload pick the class to instantiate. Use `Id.NAME` with registered subtypes.
- XML parsers resolve external entities by default (XXE). On `DocumentBuilderFactory`, `SAXParserFactory` and `XMLInputFactory`, disable DTDs (`http://apache.org/xml/features/disallow-doctype-decl`) and enable `XMLConstants.FEATURE_SECURE_PROCESSING`.
- SQL: `PreparedStatement` parameters and JPA named parameters. String concatenation into JPQL (`"... where name = '" + name + "'"`) is as injectable as SQL.
- Paths: `base.resolve(input).normalize()` followed by a `startsWith(base)` check; symlinks need `toRealPath()` or `LinkOption.NOFOLLOW_LINKS` on top.
- Tokens come from `SecureRandom`; `Random` and `ThreadLocalRandom` are predictable. Compare secrets with `MessageDigest.isEqual` (constant time).
- `Runtime.exec(String)` splits on whitespace. Use `ProcessBuilder` with an argument list and keep user input out of `sh -c`.

### Performance

- Measure with JMH; a hand-written timing loop measures JIT warm-up and dead-code elimination. Profile real workloads with JFR (`-XX:StartFlightRecording`) and JDK Mission Control or async-profiler.
- Autoboxing in hot loops (`Map<Long, Long>` counters, `Stream<Integer>`) allocates per element. Use `IntStream`/`LongStream`, primitive arrays or primitive collections.
- Presize collections when the count is known: `HashMap.newHashMap(n)` (Java 19) accounts for the load factor; `new HashMap<>(n)` does not.
- In containers the JVM reads cgroup limits. Size the heap as a share of the limit (`-XX:MaxRAMPercentage=75`); a fixed `-Xmx` has to be kept in step with the container limit by hand.
- SLF4J placeholders (`log.debug("loaded {}", id)`) skip formatting when the level is off; string concatenation in the call always runs.
- `finalize` is deprecated for removal. Release resources with try-with-resources, or `Cleaner` as a safety net.

## Verification Checklists

**Before marking Java work done:**

- [ ] `./gradlew check` or `./mvnw -q verify` passes, with `-Xlint:all -Werror` and the project's static analysis
- [ ] Formatter applied (`spotlessApply` or the project equivalent)
- [ ] New behaviour has tests; the test report shows a non-zero count for the new classes
- [ ] No `==` on boxed numbers or strings; `BigDecimal` compared with `compareTo`
- [ ] Every wrapping exception passes its cause; no empty `catch`; `InterruptedException` restores the flag
- [ ] `Optional` only as a return type; public boundaries reject null or are `@Nullable`-annotated
- [ ] `Collectors.toMap` calls have a merge function where keys can repeat
- [ ] No iteration-order assumptions on `Map.of`/`Set.of`/`HashMap` in output or tests
- [ ] Untrusted input never reaches `ObjectInputStream`, default typing, a DTD-enabled XML parser or concatenated SQL/JPQL

**Concurrency and Spring review:**

- [ ] Blocking IO on virtual threads, CPU work on a bounded platform pool; downstream concurrency bounded by a `Semaphore`
- [ ] Every `CompletableFuture` chain has an executor and an error handler
- [ ] Shared mutable state uses `ConcurrentHashMap.merge`/`compute`, atomics or locks; no check-then-act across two calls
- [ ] No `@Transactional`/`@Async`/`@Cacheable` method reached through a self-call; rollback rules cover checked exceptions
- [ ] `spring.jpa.open-in-view=false`; no N+1 queries in new repository calls; entities keep Lombok `@Data` off
