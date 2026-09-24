---
name: loom-csharp
description: C# language expertise for idiomatic, production-quality code.
triggers:
  - csharp
  - "c#"
  - dotnet
  - .net
  - nuget
  - msbuild
  - csproj
  - sln
  - xunit
  - nunit
  - mstest
  - asp.net core
  - minimal api
  - entity framework
  - ef core
  - linq
  - ConfigureAwait
  - nullable reference types
  - record struct
  - IAsyncEnumerable
  - CancellationToken
  - HttpClientFactory
  - Span
---

# C# Language Expertise

## Overview

Idiomatic, production-quality C# on current .NET (.NET 8 and .NET 10 are the LTS releases; C# 12 ships with .NET 8, C# 14 with .NET 10). Assumes competence with the language. The value is in what compiles cleanly and still fails: nullable annotations that do not hold at runtime, sync-over-async deadlocks, deferred LINQ queries that run twice or on the client, DI lifetime mismatches, and test filters that select nothing.

## Tooling

The `dotnet` CLI is the build tool, package manager, test driver and formatter front end. Keep the repository's SDK pin (`global.json`), solution layout and package management style.

| Task | Command |
| --- | --- |
| New solution and projects | `dotnet new sln -n Spool`, `dotnet new classlib -o src/Spool`, `dotnet new xunit -o tests/Spool.Tests` |
| Wire them together | `dotnet sln add src/Spool/Spool.csproj`, `dotnet add tests/Spool.Tests/Spool.Tests.csproj reference src/Spool/Spool.csproj` |
| Add a package | `dotnet add src/Spool/Spool.csproj package Polly` (never hand-edit `PackageReference`) |
| Build / test | `dotnet build`, `dotnet test` (restores and builds first) |
| Format | `dotnet format` (whitespace, code style and analyzer fixes driven by `.editorconfig`) |
| Audit dependencies | `dotnet list package --vulnerable --include-transitive` |
| Local tools | `dotnet tool restore` (versions pinned in `.config/dotnet-tools.json`) |
| Ignore build output | `dotnet new gitignore` (covers `bin/` and `obj/`) |

Shared settings go in `Directory.Build.props` at the repository root; every project below it inherits them:

```xml
<Project>
  <PropertyGroup>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
    <AnalysisLevel>latest-recommended</AnalysisLevel>
    <EnforceCodeStyleInBuild>true</EnforceCodeStyleInBuild>
    <RestorePackagesWithLockFile>true</RestorePackagesWithLockFile>
  </PropertyGroup>
</Project>
```

- Central Package Management: `Directory.Packages.props` with `<ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>` holds every `PackageVersion`; project files carry versionless `PackageReference`s, so every project in the solution resolves the same version of a package.
- NuGet audit runs during restore and reports known-vulnerable packages as `NU1901`–`NU1904` warnings; with `TreatWarningsAsErrors` they fail the build. CI restores with `dotnet restore --locked-mode` so a drifted `packages.lock.json` fails.
- `dotnet format` applies `.editorconfig` rules; some repositories use CSharpier for layout instead. Follow what the repository has.
- `EnforceCodeStyleInBuild` makes IDE style rules (`IDExxxx`) build diagnostics, so the build and the IDE agree.

The gate (warnings are errors through `Directory.Build.props`):

```bash
dotnet format --verify-no-changes && dotnet build && dotnet test --no-build
```

## Nullable Reference Types

Nullable annotations are a compile-time analysis. Nothing checks them at runtime: a `string` parameter can still receive `null` from reflection, deserialization, an older assembly, or a caller that wrote `null!`.

- Enable per project (`<Nullable>enable</Nullable>`) and make the warnings errors (`TreatWarningsAsErrors`, or `<WarningsAsErrors>nullable</WarningsAsErrors>` alone). Nullable warnings left as warnings pile up into the hundreds and stop being read.
- Check at public boundaries: `ArgumentNullException.ThrowIfNull(arg)`, `ArgumentException.ThrowIfNullOrWhiteSpace(name)` (.NET 8), `ArgumentOutOfRangeException.ThrowIfNegative(count)` (.NET 8). They capture the argument name through `CallerArgumentExpression`.
- `!` (null-forgiving) silences the compiler and changes nothing at runtime. Each one is a claim the reviewer must be able to check from the surrounding lines; `= default!` on a property hands the lie to the next reader.
- Teach the analysis with `System.Diagnostics.CodeAnalysis` attributes so callers need no `!`:

```csharp
public bool TryGetSpool(string id, [NotNullWhen(true)] out Spool? spool)
{
    spool = _spools.GetValueOrDefault(id);
    return spool is not null;
}

if (TryGetSpool(id, out var spool))
{
    spool.Append(record); // no warning: NotNullWhen(true) proved it non-null
}
```

- Other attributes: `[MemberNotNull(nameof(_conn))]` on an initialization helper, `[return: NotNullIfNotNull(nameof(input))]`, `[DoesNotReturn]` on throw helpers.
- Holes the analysis does not see: `new string[4]` holds four nulls typed `string`; `default(SomeStruct)` leaves its reference fields null; a property initialized by neither constructor nor `required`.
- System.Text.Json writes `null` into non-nullable properties unless told otherwise. Mark mandatory members `required` (honoured since .NET 7: a missing property throws) and set `JsonSerializerOptions.RespectNullableAnnotations = true` (.NET 9) to reject explicit `null`s.
- Unconstrained generic `T?` means "may be `default`": for `T = int` it is `int`, with no `Nullable<int>` involved. Constrain with `where T : class` or `where T : struct` when the distinction matters.

## Records, Structs and Classes

| Need | Use |
| --- | --- |
| Immutable data with value equality | `record` (reference) or `readonly record struct` (small value) |
| Identity, behaviour, mutable state | `sealed class` (unsealed only when designed for inheritance) |
| Small (about 16 bytes or less), short-lived value, no boxing | `readonly struct` |
| Members that must be set at construction | `required` (C# 11) + `init` (C# 9) |

```csharp
public sealed record SpoolEntry(string Id, DateTimeOffset At, IReadOnlyList<string> Tags);

var moved = entry with { At = clock.GetUtcNow() }; // shallow copy: Tags is the same list object

public sealed class SpoolOptions
{
    public required string Root { get; init; }
    public int MaxBytes { get; init; } = 1 << 20;
}
```

- ⚠ **Record equality compares each member with `EqualityComparer<T>.Default`.** A `List<T>`, array or `ImmutableArray<T>` member compares by reference, so two records with equal contents are unequal: dictionary lookups miss and test assertions fail. Override `Equals(SpoolEntry? other)` and `GetHashCode` with `SequenceEqual`, or keep collections out of records used as keys or compared in tests.
- `record struct` positional properties are **mutable** (`get; set;`); `readonly record struct` makes them `init`.
- `with` copies shallowly; nested mutable objects are shared by both copies.
- Primary constructors on classes (C# 12) capture parameters as hidden mutable fields; they are not properties. Initializing a property from a parameter and also reading the parameter in a method stores two copies that can diverge (warning CS9124). Assign to one `readonly` member and use only that.
- `sealed` lets the JIT devirtualize calls; CA1852 flags internal types that could be sealed.
- A non-exhaustive switch expression warns CS8509 and throws `SwitchExpressionException` at runtime. Enums are open sets (any `int` casts to an enum), so a switch naming every member still warns CS8524 until it has a `_` arm; make that arm throw.

```csharp
decimal Fee(Order o) => o switch
{
    { Items: [] } => throw new InvalidOperationException("empty order"),
    { Total: > 1000m } => 0m,
    { Customer.IsPremium: true } => 2.5m,
    _ => 5m,
};
```

- Collection expressions (C# 12): `int[] ids = [1, 2, 3];`, `List<string> all = [..left, ..right];`.
- C# 14 (.NET 10): the `field` keyword reaches the compiler-generated backing field from an accessor (`set => field = value.Trim();`); `extension` blocks declare extension properties and static members; `a?.B = x` assigns only when `a` is non-null.

## Exceptions and Disposal

- Throw for failures the caller cannot anticipate; use the `TryX(out T)` pattern or a result type for expected outcomes (parse failures, cache misses). Exceptions cost microseconds each and must not drive a hot loop.
- Rethrow with `throw;`. `throw ex;` resets the stack trace to the rethrow line. To rethrow a captured exception elsewhere, `ExceptionDispatchInfo.Capture(ex).Throw()`.
- Exception filters run before the stack unwinds, so crash dumps keep the throwing frame:

```csharp
try
{
    return await client.GetFromJsonAsync<Blob>(uri, ct);
}
catch (HttpRequestException e) when (e.StatusCode == HttpStatusCode.NotFound)
{
    return null;
}
```

- `catch (Exception)` belongs at process and request boundaries, where it logs and translates. Anywhere else it also catches `OperationCanceledException` and turns a cancellation into a phantom error or a silent success.
- Own a resource with `using var` (disposed at scope end) or `await using` for `IAsyncDisposable`. A class that owns an `IDisposable` field implements `IDisposable` itself. Finalizers only for types wrapping raw OS handles, and `SafeHandle` usually removes the need.

## Async / Await

- Async all the way down. `.Result`, `.Wait()` and `.GetAwaiter().GetResult()` on a thread with a `SynchronizationContext` (WPF, WinForms, MAUI, classic ASP.NET) deadlock: the continuation needs the context the blocked thread holds. ASP.NET Core has no context, so the same code runs there and starves the thread pool under load instead.
- `ConfigureAwait(false)` tells an await not to resume on the captured context. Use it on every await in **library** code that UI or legacy hosts may call (enable CA2007 in library projects). Application code in ASP.NET Core, console apps and worker services gains nothing from it. It applies only to the await it is attached to and does nothing when that task already completed, so a library applies it to every await.
- .NET 8 adds `ConfigureAwaitOptions` for `Task`: `await loop.ConfigureAwait(ConfigureAwaitOptions.SuppressThrowing)` waits for a cancelled background loop to finish without rethrowing.
- `async void` only for event handlers: its exceptions go to the synchronization context or crash the process, and nothing can await it.
- `ValueTask<T>` is for hot paths that usually complete synchronously. Await it exactly once; never await it twice, read `.Result` before completion, or await it concurrently. Call `.AsTask()` when you need any of those.
- `CancellationToken` is the last parameter of every async API and is passed to every call inside. For a timeout, `using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct); cts.CancelAfter(timeout);`. Undisposed linked sources leak registrations on the parent token.
- `await Task.WhenAll(tasks)` rethrows only the first exception; all of them are on the returned task's `Exception.InnerExceptions`.
- `await` inside `lock` does not compile (CS1996). Guard async critical sections with `SemaphoreSlim(1, 1)` and release in `finally`. For synchronous locks, .NET 9 / C# 13 adds `System.Threading.Lock`; `lock` on a `Lock` field uses its faster scope API.
- Bounded concurrency: `Parallel.ForEachAsync` with `MaxDegreeOfParallelism` (.NET 6); `Channel.CreateBounded<T>(capacity)` for producer/consumer with backpressure.
- `IAsyncEnumerable<T>` producers take `[EnumeratorCancellation] CancellationToken ct` so `await foreach (var x in source.WithCancellation(ct))` flows the token.
- Fire-and-forget (`_ = DoWorkAsync();`) loses exceptions and outlives the request scope. Queue the work to a `BackgroundService` through a `Channel<T>`.

```csharp
public async Task<Spool> OpenAsync(string path, CancellationToken ct)
{
    await _gate.WaitAsync(ct).ConfigureAwait(false);
    try
    {
        var bytes = await File.ReadAllBytesAsync(path, ct).ConfigureAwait(false);
        return Spool.Parse(bytes);
    }
    finally
    {
        _gate.Release();
    }
}
```

## LINQ Pitfalls

- **Deferred execution.** A query is a recipe; each enumeration runs it again (and queries the database again). Materialize once with `ToList()`/`ToArray()` when a sequence is enumerated more than once (CA1851 flags it). Side effects inside `Select` run lazily, repeatedly, or never.
- **`IQueryable<T>` and `IEnumerable<T>`.** EF Core translates the expression tree to SQL up to the first `AsEnumerable()`/`ToList()`; every operator after that runs in memory over the rows already fetched. Filter, project and page before materializing:

```csharp
// BAD: pulls every order into memory, then filters and pages
var recent = db.Orders.ToList().Where(o => o.PlacedAt > since).Take(50);

// GOOD: one SQL query with WHERE, ORDER BY and a row limit
var recent = await db.Orders
    .AsNoTracking()
    .Where(o => o.PlacedAt > since)
    .OrderByDescending(o => o.PlacedAt)
    .Select(o => new OrderRow(o.Id, o.Total))
    .Take(50)
    .ToListAsync(ct);
```

- N+1: lazy-loading proxies, or a navigation property read inside a loop, issue one query per row. Load with `Include` or project with `Select`; add `AsSplitQuery()` when several collection `Include`s multiply the row count.
- `Any()` stops at the first element; `Count() > 0` counts them all. `Single` throws on more than one match, `First` on none. `FirstOrDefault` on a value type returns `0`/`default`, indistinguishable from a real zero: pass the fallback explicitly (`FirstOrDefault(pred, -1)`, .NET 6) or use a `TryGet`-style API.
- `OrderBy(a).OrderBy(b)` discards the first sort; chain `ThenBy`.
- A `for` loop variable is shared by every lambda created in the loop; a `foreach` variable is per iteration (since C# 5).
- Built-in operators replace hand-written grouping: `Chunk`, `DistinctBy`, `MaxBy`/`MinBy` (.NET 6); `CountBy`, `AggregateBy`, `Index` (.NET 9).
- LINQ allocates enumerators and delegates. In a measured hot path, loop over an array or span.

## ASP.NET Core Minimal APIs

```csharp
var builder = WebApplication.CreateBuilder(args);
builder.Services.AddProblemDetails();
builder.Services.AddDbContext<SpoolDb>(o => o.UseNpgsql(builder.Configuration.GetConnectionString("spool")));
builder.Services.AddHttpClient<BlobClient>(c => c.BaseAddress = new Uri("https://blobs.internal/"));
builder.Services.AddOptions<SpoolOptions>().BindConfiguration("Spool").ValidateDataAnnotations().ValidateOnStart();

var app = builder.Build();
app.UseExceptionHandler();

var spools = app.MapGroup("/spools").RequireAuthorization();
spools.MapGet("/{id:guid}", GetSpool);
app.Run();

static async Task<Results<Ok<SpoolDto>, NotFound>> GetSpool(Guid id, SpoolDb db, CancellationToken ct)
{
    var spool = await db.Spools.AsNoTracking().FirstOrDefaultAsync(s => s.Id == id, ct);
    return spool is null ? TypedResults.NotFound() : TypedResults.Ok(SpoolDto.From(spool));
}
```

- `TypedResults` with a `Results<...>` return type gives the handler a checkable signature and generates OpenAPI metadata; a handler written as a static method is unit-testable without a server.
- Binding: route and query parameters by name, complex types from the JSON body, registered services from DI, `CancellationToken` from `HttpContext.RequestAborted`; `[AsParameters]` groups them into one record.
- Validation: .NET 10 adds `builder.Services.AddValidation()`, which runs DataAnnotations attributes on minimal API parameters; earlier versions need an endpoint filter or FluentValidation. An unvalidated body model is untrusted input.
- DI lifetimes: a singleton that takes a scoped service (a `DbContext`) keeps one instance forever, a captive dependency with stale data and cross-request state. Development builds validate scopes by default; turn on `ValidateOnBuild` and `ValidateScopes` in tests too. A `BackgroundService` creates scopes through `IServiceScopeFactory`.
- `HttpClient`: `new HttpClient()` per call exhausts sockets; one static instance never sees DNS changes unless `SocketsHttpHandler.PooledConnectionLifetime` is set. Use `IHttpClientFactory` (typed clients via `AddHttpClient<T>`).
- `ValidateOnStart()` on options makes bad configuration fail at startup instead of on the first request that reads it.
- `AddProblemDetails()` plus `UseExceptionHandler()` turn unhandled exceptions into problem-details JSON without leaking stack traces.

## Testing

xUnit and NUnit differ in the place that causes most flaky suites: instance lifetime.

| Concern | xUnit | NUnit |
| --- | --- | --- |
| Test | `[Fact]`; data rows `[Theory]` + `[InlineData]`/`[MemberData]` | `[Test]`; data rows `[TestCase(...)]`/`[TestCaseSource]` |
| Setup / teardown | constructor / `Dispose` (`IAsyncLifetime` for async) | `[SetUp]` / `[TearDown]`, `[OneTimeSetUp]` |
| Instance per test | yes: a new class instance for every test | no: one fixture instance shared by its tests (`[FixtureLifeCycle(LifeCycle.InstancePerTestCase)]` changes it) |
| Shared expensive state | `IClassFixture<T>`, `ICollectionFixture<T>` | `[OneTimeSetUp]` fields, `[SetUpFixture]` |
| Parallelism | classes (collections) in parallel, tests within a class in sequence | off unless `[Parallelizable]` |
| Assertions | `Assert.Equal(expected, actual)` | `Assert.That(actual, Is.EqualTo(expected))`; classic asserts live in `ClassicAssert` since NUnit 4 |

```csharp
public sealed class SpoolWriterTests : IClassFixture<TempDirFixture>
{
    private readonly TempDirFixture _dir;

    public SpoolWriterTests(TempDirFixture dir) => _dir = dir;

    [Fact]
    public async Task AppendsRecordToJournal()
    {
        var writer = new SpoolWriter(_dir.Path, new FakeTimeProvider());
        await writer.AppendAsync("a", TestContext.Current.CancellationToken);
        Assert.Single(await File.ReadAllLinesAsync(writer.JournalPath));
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    public void RejectsBlankIds(string id) =>
        Assert.Throws<ArgumentException>(() => SpoolId.Parse(id));
}
```

- xUnit v3 test projects build to executables and expose `TestContext.Current` (cancellation token, output); `async void` test methods are not supported, return `Task`. `ITestOutputHelper` (constructor-injected) captures per-test output.
- Time and randomness through seams: inject `TimeProvider` (.NET 8) and advance a `FakeTimeProvider` (`Microsoft.Extensions.TimeProvider.Testing`); code under test never reads `DateTime.Now`.
- Prefer fakes and real in-memory collaborators; mock (NSubstitute, Moq) only true external boundaries. For databases, run the production engine in Testcontainers: the EF Core in-memory provider enforces no constraints and translates no SQL, so it accepts queries production rejects.
- HTTP: `WebApplicationFactory<Program>` hosts the app in memory; add `public partial class Program { }` when the test project cannot see the generated `Program`.
- Assertion libraries: FluentAssertions 8 and later needs a paid license for commercial use; Shouldly and AwesomeAssertions (a fork of FluentAssertions 7) are open. The built-in `Assert` covers most suites.
- Filters: `dotnet test --filter "FullyQualifiedName~SpoolWriter"` (`~` is contains), `--filter "Category=Integration"` (xUnit `[Trait("Category", "Integration")]`, NUnit `[Category("Integration")]`), `--no-build` after a separate build, `--logger "console;verbosity=detailed"`.

## Loom Test Runner Adapter

**Adapter:** `dotnet-test`. Loom marks a package `csharp` when its directory holds any `*.csproj` or `*.sln`, and every `csharp` package gets `dotnet-test`. `loom project detect` prints it per package:

```text
tests/Spool.Tests  kinds=csharp  runner=dotnet-test  skills=loom-csharp
```

A directory holding only a `.slnx` solution is not a `csharp` package under that rule; each project directory beneath it still is.

**Single-test command**, run with the package directory as cwd (`dotnet test` restores and builds before it runs anything):

```bash
dotnet test --filter "FullyQualifiedName={test}"
```

**The `test` field** is `Namespace.Class.Method`, the test's fully qualified name. `=` is an exact match, so the value is the whole name: `Spool.Tests.SpoolWriterTests.RejectsSymlinkedSpool`. The command does not use the contract's `file`; loom uses it to find the owning package and to freeze the file.

**No-match behaviour:** when the filter selects nothing, `dotnet test` exits **0** and prints `No test matches the given testcase filter ...` on stdout (captured from a real run: xUnit v3, .NET SDK 10.0.112, VSTest 18). Loom reads the runner's summary (`Passed!  - Failed:     0, Passed:     1, ...` or `Failed!  - ...`), so a contract test whose name does not match is classified as not selected and fails the freeze ("the runner did not select the test"). Completion fails the same way.

**Writing contract tests:**

- Contract tests live in a test project, conventionally `tests/<Project>.Tests/`, one `<Subject>Tests.cs` per subject; loom's `csharp` language profile decides which paths count as test files. SDK-style projects compile every `*.cs` under the project directory, so a new test file in an existing test project needs no project-file edit. A new test project (its `.csproj`, the solution entry, `Directory.Packages.props` changes) is harness: list those paths in the stage's `harness` globs.
- Make each contract a parameterless `[Fact]` (xUnit) or `[Test]` (NUnit) in a top-level public class, with a method name unique in that class. Nested classes put `+` into the fully qualified name (`Outer+Inner.Method`). `[Theory]` rows share one name and NUnit appends each `[TestCase]` row's arguments to it, so an exact filter selects several tests or none.
- Keep one solution or project file per package directory. With two (a `.sln` beside a `.csproj`, or two `.csproj` files) `dotnet test` stops with `MSB1011` before building anything.
- `bin/` and `obj/` must be gitignored. The freeze rejects any changed or untracked path that is neither a contract `file` nor a `harness` match, and `dotnet test` creates both directories.
- The command and its parser target `dotnet test`'s default VSTest mode. A repository whose `global.json` switches `dotnet test` to Microsoft.Testing.Platform prints a different summary; run the command by hand once and confirm the `Passed!`/`Failed!` line appears before relying on it.
- Set `runner: dotnet-test` on a contract only to override detection.

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: tests/Spool.Tests/SpoolWriterTests.cs
    test: Spool.Tests.SpoolWriterTests.RejectsSymlinkedSpool
    scenario: creates a spool directory whose journal file is a symlink to a file outside the spool root, then calls SpoolWriter.OpenAsync on the directory and appends one record
    rejects: an OpenAsync that follows the symlink and appends the record to the file outside the spool root
```

**Build failures:** `dotnet test` compiles the project before running anything. A contract test that calls a type or member the stage has not written yet fails compilation (`CS0246`, `CS1061`); loom classifies the run as a build failure, and the freeze accepts that as red. Any other compile error in the project is also a build failure, so read the diagnostics and confirm they name the API the contract is waiting for.

## Anti-Patterns

```csharp
// Sync over async: deadlocks under a SynchronizationContext, starves the pool elsewhere
var data = client.GetStringAsync(url).Result;                        // BAD
var data = await client.GetStringAsync(url, ct);                     // GOOD

// async void outside event handlers: exceptions crash the process
public async void Save() { await _repo.SaveAsync(); }                // BAD
public Task SaveAsync(CancellationToken ct) => _repo.SaveAsync(ct);  // GOOD

// Rethrow that destroys the stack trace
catch (IOException ex) { Log(ex); throw ex; }                        // BAD
catch (IOException ex) { Log(ex); throw; }                           // GOOD

// Culture-sensitive comparison of machine data (tr-TR: "TITLE".ToLower() is "tıtle")
if (header.ToLower() == "title") { }                                 // BAD
if (string.Equals(header, "Title", StringComparison.OrdinalIgnoreCase)) { } // GOOD

// Interpolated log message: always allocates, loses the structured field (CA2254)
logger.LogInformation($"Opened {path}");                             // BAD
logger.LogInformation("Opened {Path}", path);                        // GOOD
```

Quick swaps:

- `DateTime.Now` in domain logic → an injected `TimeProvider`; `DateTime` for instants → `DateTimeOffset` in UTC.
- `new Random()` for tokens or IDs → `RandomNumberGenerator.GetBytes`/`GetInt32`.
- Public `List<T>` properties → `IReadOnlyList<T>`; `null` for "no items" → an empty collection.
- `lock (this)` or `lock (typeof(X))` → a private `readonly Lock` (or `object`) field.
- String concatenation in a loop → `StringBuilder` or `string.Join`.
- `double` for money → `decimal`.
- `catch (Exception) { }` → the specific exception type, or let it propagate.
- `GC.Collect()` → delete it and measure allocations.

## Expert Practices

### Performance

- Measure before optimizing: BenchmarkDotNet with `[MemoryDiagnoser]` for allocations; `dotnet-counters` and `dotnet-trace` against a running process.
- `Span<T>`/`ReadOnlySpan<char>` parse and slice without allocating; `stackalloc` for small buffers; `ArrayPool<T>.Shared` for large temporary buffers (return them in `finally` and never touch them after). A `Span<T>` is a `ref struct` and cannot live across an `await`; async code uses `Memory<T>`.
- `FrozenDictionary`/`FrozenSet` (.NET 8) for lookups built once and read often.
- Source generators remove reflection and keep trimming and Native AOT working: `[GeneratedRegex]` (.NET 7), `[LoggerMessage]` (.NET 6), `JsonSerializerContext` for System.Text.Json.
- A struct passed as an interface or `object` is boxed (an allocation); generic constraints (`where T : IComparable<T>`) avoid it.

### Correctness Gotchas

- `ConcurrentDictionary.GetOrAdd(key, factory)` can run the factory more than once under contention (one result is kept). Store `Lazy<T>` values when the factory is expensive or has side effects.
- Override `Equals` and `GetHashCode` together (`HashCode.Combine`), and never mutate a field that feeds the hash of an object used as a dictionary key.
- Parse and format machine data with `CultureInfo.InvariantCulture`: under `de-DE`, `double.Parse("1.5")` returns `15`.
- A `static` field of a generic class exists once per closed type: `Cache<int>` and `Cache<string>` share nothing.
- Calling a non-`readonly` member on a `readonly` field or an `in` parameter copies the struct first (a defensive copy), and the mutation is lost. Mark structs and their members `readonly`.
- An `IEnumerable<T>` parameter may be lazy, infinite or single-use; take `IReadOnlyCollection<T>`/`IReadOnlyList<T>` when the method needs a count or a second pass.

### Security

- SQL: EF Core `FromSql($"... {id}")` and `ExecuteSql` turn interpolations into parameters; `FromSqlRaw` over concatenated input is injection. Dapper and ADO.NET take parameters, always.
- Deserialization: `BinaryFormatter` is gone (.NET 9 throws). Newtonsoft.Json with any `TypeNameHandling` other than `None` lets the payload choose the type to instantiate, a known remote-code-execution route. System.Text.Json polymorphism uses an explicit `[JsonDerivedType]` allow-list.
- Paths: `Path.Combine(root, input)` returns `input` alone when it is rooted, and `..` segments walk out of the root. Resolve with `Path.GetFullPath(Path.Join(root, input))` and check the result starts with the root plus a directory separator.
- Secrets: user-secrets in development, a vault or the environment in production, never `appsettings.json`. Generate tokens with `RandomNumberGenerator` and compare them with `CryptographicOperations.FixedTimeEquals`.
- Processes: `ProcessStartInfo` with `ArgumentList` (each argument escaped on its own) and `UseShellExecute = false`; never assemble a shell command string from input.

## Verification Checklists

**Before marking C# work done:**

- [ ] `dotnet format --verify-no-changes` clean; `dotnet build` with `TreatWarningsAsErrors` has zero warnings (nullable, analyzers, NuGet audit)
- [ ] `dotnet test` green; new behaviour has tests; contract tests are parameterless `[Fact]`/`[Test]` methods in top-level classes
- [ ] Every `!` and `default!` has a reason visible in the surrounding lines; public entry points guard arguments with `ThrowIfNull` and its siblings
- [ ] Records used as keys or compared in tests hold no collection members (or override equality)
- [ ] Packages added with `dotnet add package`; central versions and `packages.lock.json` updated with them
- [ ] No hardcoded secrets; SQL parameterized; no `TypeNameHandling`; user-supplied paths resolved and prefix-checked
- [ ] `bin/` and `obj/` ignored by git

**Async and data access review:**

- [ ] No `.Result`, `.Wait()` or `GetAwaiter().GetResult()`; no `async void` outside event handlers
- [ ] Library awaits use `ConfigureAwait(false)`; every async API takes and forwards a `CancellationToken`
- [ ] No `await` under `lock`; every `SemaphoreSlim.WaitAsync` has its `Release` in `finally`
- [ ] Queries enumerated once; EF filters and projections run before materializing; no N+1 loops; read-only queries use `AsNoTracking()`
- [ ] No scoped service captured by a singleton; `HttpClient` instances come from `IHttpClientFactory`
