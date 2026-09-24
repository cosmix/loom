---
name: loom-swift
description: Swift language expertise for idiomatic, production-quality code.
triggers:
  - swift
  - swiftpm
  - spm
  - Package.swift
  - xcode
  - xcodebuild
  - xctest
  - swift testing
  - swift-format
  - swiftlint
  - swiftui
  - uikit
  - vapor
  - sendable
  - actor
  - mainactor
  - async let
  - taskgroup
  - codable
  - ios
  - macos
---

# Swift Language Expertise

## Overview

Idiomatic, production-grade Swift 6: SwiftPM packages, value semantics, optionals, protocol-oriented generics, and structured concurrency under strict data-race checking. Assumes the reader knows the syntax. The value is in what is easy to get wrong: protocol-extension dispatch, actor reentrancy, `Sendable` escape hatches, continuation misuse, `Codable` defaults, and two test frameworks that share one `swift test` command.

## Tooling

SwiftPM is the build tool and package manager. Xcode projects (`.xcodeproj`) build with `xcodebuild`; keep libraries in a SwiftPM package even when an app wraps them, so they build and test on Linux and in CI without a simulator.

| Task | Command |
| --- | --- |
| New package | `swift package init --type library` (or `--type executable`) |
| Add a dependency | `swift package add-dependency https://github.com/apple/swift-argument-parser --from 1.5.0` (6.0+) |
| Wire it to a target | `swift package add-target-dependency ArgumentParser spool-cli --package swift-argument-parser` |
| Resolve / update | `swift package resolve` / `swift package update` (rewrites `Package.resolved`) |
| Build with tests | `swift build --build-tests` |
| Test | `swift test` (`--parallel` for XCTest; Swift Testing runs tests in parallel by default) |
| List test ids | `swift test list` |
| Release build | `swift build -c release` (whole-module optimization) |
| Format | `swift format --in-place --recursive Sources Tests` (swift-format ships with the 6.0 toolchain) |
| Lint | `swift format lint --strict --recursive Sources Tests`; `swiftlint lint --strict` where `.swiftlint.yml` exists |

```swift
// swift-tools-version: 6.0
// The tools version line selects the manifest API AND the Swift 6 language mode for every target.
import PackageDescription

let package = Package(
    name: "Spool",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [.library(name: "Spool", targets: ["Spool"])],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser", from: "1.5.0"),
    ],
    targets: [
        .target(name: "Spool"),
        .executableTarget(name: "spool-cli", dependencies: [
            "Spool",
            .product(name: "ArgumentParser", package: "swift-argument-parser"),
        ]),
        .testTarget(name: "SpoolTests", dependencies: ["Spool"]),
    ]
)
```

- Commit `Package.resolved` for applications and executables. Libraries may leave it out: consumers resolve their own graph.
- Migrating an older target: `swiftSettings: [.swiftLanguageMode(.v5), .enableUpcomingFeature("StrictConcurrency")]` keeps Swift 5 mode while surfacing the data-race diagnostics as warnings; remove both once they are fixed.
- Keep executables thin. Logic lives in a library target so tests `@testable import` it.

The canonical gate:

```bash
swift format lint --strict --recursive Sources Tests
swift build --build-tests -Xswiftc -warnings-as-errors
swift test --parallel
```

If `-warnings-as-errors` trips on a dependency's code, drop the flag and fail CI on `warning:` lines under your own `Sources/` and `Tests/` instead.

## Value Types and Copy-on-Write

- Default to `struct` and `enum`. Use `class` when identity or shared mutable state is the point (a connection, a cache, a UIKit object). A struct copy mutates independently, which removes aliasing bugs and makes `Sendable` conformance free when every stored property is `Sendable`.
- A struct holding a class reference copies the reference. Both copies share the object; value semantics stop at that field.
- `Array`, `Dictionary`, `Set` and `String` are copy-on-write: a copy is O(1) and the first mutation of a shared buffer copies it. A struct that wraps its own class storage must implement CoW itself.
- Exclusivity: overlapping `inout` access to one variable is a compile error when the compiler can see it and a runtime trap ("Simultaneous accesses") when it cannot.

```swift
final class Storage { var bytes: [UInt8] = [] }

struct Buffer {
    private var storage = Storage()

    mutating func append(_ byte: UInt8) {
        if !isKnownUniquelyReferenced(&storage) {   // shared: copy before writing
            let copy = Storage()
            copy.bytes = storage.bytes
            storage = copy
        }
        storage.bytes.append(byte)
    }
}

var counts = [1, 2, 3]
swap(&counts[0], &counts[1])   // error: overlapping accesses to 'counts'
counts.swapAt(0, 1)            // the API for this case
```

## Optionals and Errors

- Unwrap with `if let value`, `guard let value else { return }` (5.7 shorthand), `??` and optional chaining. `guard` keeps the success path unindented.
- `!`, `try!` and `as!` assert a fact the program itself guarantees. Data from outside the program (environment, files, network, user input) is never such a fact.
- Model failures as an `enum` conforming to `Error` with associated values. `try?` discards the reason; use it only where the reason does not matter.
- Typed throws (6.0): `throws(SpoolError)` fits module-internal code and closed error sets. Keep untyped `throws` on public API: a typed error set is part of the signature and cannot grow without breaking callers.
- `precondition` and `fatalError` survive `-O`; `assert` is compiled out of release builds. Never validate input with `assert`.

```swift
enum ConfigError: Error, Equatable {
    case missing(key: String)
    case invalid(key: String, value: String)
}

func port(from env: [String: String]) throws(ConfigError) -> Int {
    guard let raw = env["PORT"] else { throw .missing(key: "PORT") }
    guard let value = Int(raw), (1...65_535).contains(value) else {
        throw .invalid(key: "PORT", value: raw)
    }
    return value
}

do {
    let p = try port(from: ProcessInfo.processInfo.environment)
    print("listening on \(p)")
} catch ConfigError.missing(let key) {
    print("set \(key)")
} catch {
    print("bad config: \(error)")
}
```

## Protocols and Generics

- `some P` is one concrete type chosen at compile time: static dispatch, specialization, no boxing. `any P` is an existential box with dynamic dispatch and possible heap allocation. Default to `some P` or a generic parameter; use `any P` for heterogeneous storage.
- Primary associated types constrain both forms: `some Collection<Int>`, `any AsyncSequence<Event, Never>`.
- Conditional conformance: `extension Array: Drawable where Element: Drawable {}`.
- Conforming a type you do not own to a protocol you do not own warns in Swift 6; mark it `extension URL: @retroactive Identifiable` only when you accept that the owner may add the same conformance later.
- ⚠ Only protocol **requirements** dispatch dynamically. A method defined only in a protocol extension is picked by the static type, so a conforming type's same-named method is ignored through `any P` or a generic `T: P`.

```swift
protocol Greeter {
    func greet() -> String                  // requirement: dynamic dispatch
}

extension Greeter {
    func greet() -> String { "hello" }
    func farewell() -> String { "bye" }     // extension only: static dispatch
}

struct Loud: Greeter {
    func greet() -> String { "HELLO" }
    func farewell() -> String { "BYE" }
}

let g: any Greeter = Loud()
g.greet()      // "HELLO"
g.farewell()   // "bye": the extension method, chosen at compile time
```

## Structured Concurrency

- `async let` runs a fixed number of children concurrently; `withThrowingTaskGroup` runs a dynamic number. When the scope exits by error, remaining children are cancelled and awaited.
- Cancellation is cooperative. Check `try Task.checkCancellation()` in loops; `Task.sleep` throws `CancellationError`.
- An unstructured `Task { }` inherits the caller's actor and priority but no cancellation. `Task.detached` inherits nothing. Keep the handle of every unstructured task and cancel it when its owner goes away.
- Actors serialize access to their state, and they are **reentrant**: at every `await` other calls may run and change that state. Re-check invariants after each suspension.
- `@MainActor` isolates UI state and view models.
- `Sendable`: structs and enums whose members are `Sendable` conform implicitly (inside the module). A class conforms when it is `final` with only `let` `Sendable` properties, or when a lock guards its state and it is marked `@unchecked Sendable` with a comment naming the lock. `Mutex` from the `Synchronization` module (6.0, macOS 15 / iOS 18) is that lock.
- Swift 6 language mode turns data-race diagnostics into errors. `@preconcurrency import LegacyKit` quiets diagnostics from a module that has not adopted `Sendable` yet.
- Swift 6.2 adds `.defaultIsolation(MainActor.self)` per target and the `NonisolatedNonsendingByDefault` upcoming feature, under which a nonisolated `async` function runs on the caller's actor unless marked `@concurrent`. Read the target's `swiftSettings` before reasoning about where an async function runs.

```swift
func loadDashboard(user: User.ID) async throws -> Dashboard {
    async let profile = api.profile(user)      // both requests start here
    async let orders = api.orders(user)
    return try await Dashboard(profile: profile, orders: orders)
}

func fetchAll(_ ids: [Item.ID]) async throws -> [Item] {
    try await withThrowingTaskGroup(of: Item.self) { group in
        for id in ids {
            group.addTask { try await api.item(id) }
        }
        var items: [Item] = []
        for try await item in group { items.append(item) }
        return items                             // a child's error cancels the rest
    }
}

actor DownloadCache {
    private var cache: [URL: Data] = [:]
    private var inFlight: [URL: Task<Data, Error>] = [:]

    func data(for url: URL) async throws -> Data {
        if let hit = cache[url] { return hit }
        if let running = inFlight[url] { return try await running.value }
        let task = Task { try await download(url) }
        inFlight[url] = task                     // record before suspending
        defer { inFlight[url] = nil }
        let data = try await task.value          // other callers run during this await
        cache[url] = data
        return data
    }
}
```

Bridging callback APIs: a checked continuation must be resumed exactly once on every path. Resuming twice traps; never resuming leaks the task and logs `SWIFT TASK CONTINUATION MISUSE`.

```swift
func fetch(_ url: URL) async throws -> Data {
    try await withCheckedThrowingContinuation { continuation in
        legacyClient.fetch(url) { result in
            continuation.resume(with: result)    // Result<Data, Error>: one resume on every path
        }
    }
}
```

## Modules and Access Control

- Each target is a module. `internal` (the default) is module-wide; `public` exposes API; `open` also allows subclassing and overriding outside the module; `package` (5.9) spans the modules of one package; `fileprivate` and `private` narrow further.
- Tests reach `internal` symbols with `@testable import Spool`. Do not widen access for a test.
- Resources: `resources: [.process("Fixtures")]` on the target, read through `Bundle.module`.
- `@unknown default` in a `switch` over an enum from another module (SDK enums, resilient libraries) keeps the code compiling when the enum gains a case, and warns so you handle it.

## SwiftUI Essentials

- Views are values that SwiftUI recreates often. Keep `body` cheap and free of side effects.
- State ownership: `@State` for view-owned values and for an `@Observable` model the view creates; `@Binding` for a child that writes its parent's value; `@Bindable` for bindings into an `@Observable` model; `@Environment` for injected shared objects. `@StateObject` and `@ObservedObject` remain for `ObservableObject` before iOS 17.
- `.task { }` runs async work tied to the view's lifetime and cancels it when the view disappears; `.task(id:)` restarts when the id changes. Prefer it to `onAppear { Task { } }`, which never cancels.

```swift
import Observation
import SwiftUI

@MainActor
@Observable
final class SpoolModel {
    var entries: [String] = []
    var failure: String?

    func reload(client: SpoolClient) async {
        do { entries = try await client.list() } catch { failure = error.localizedDescription }
    }
}

struct SpoolView: View {
    let client: SpoolClient
    @State private var model = SpoolModel()

    var body: some View {
        List(model.entries, id: \.self) { Text($0) }
            .overlay { if let failure = model.failure { Text(failure) } }
            .task { await model.reload(client: client) }   // cancelled when the view goes away
    }
}
```

## Testing

Two frameworks run under one `swift test`:

| | XCTest | Swift Testing (6.0 toolchain) |
| --- | --- | --- |
| Declare | `final class FooTests: XCTestCase` with `func testX()` | `@Test func x()`, optionally in a `@Suite` type |
| Assert | `XCTAssertEqual`, `XCTAssertThrowsError`, `XCTUnwrap` | `#expect(a == b)`, `#expect(throws:)`, `try #require(x)` |
| Setup | `setUpWithError` / `tearDownWithError`, `addTeardownBlock` | the suite's `init` (a fresh instance per test), `deinit` on a class suite |
| Parameterize | loop inside one test | `@Test(arguments: [...])`, one result per argument |
| Parallelism | serial unless `--parallel` | parallel by default; `.serialized` trait to opt out |

```swift
import XCTest
@testable import Spool

final class SpoolWriterTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try FileManager.default.removeItem(at: dir)
    }

    func testAppendsOneRecord() throws {
        let writer = try SpoolWriter(path: dir.appendingPathComponent("spool").path)
        try writer.append(Data("x".utf8))
        let segment = try XCTUnwrap(writer.segments.first)
        XCTAssertEqual(segment.size, 1)
    }

    func testDrainsAsync() async throws {
        let writer = try SpoolWriter(path: dir.appendingPathComponent("spool").path)
        let drained = try await writer.drain()
        XCTAssertEqual(drained, 0)
    }
}
```

```swift
import Foundation
import Testing
@testable import Spool

@Suite struct RotationTests {
    let dir: URL

    init() throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    @Test(arguments: [1, 64, 4096])
    func rotatesAtLimit(limit: Int) throws {
        let spool = try Spool(directory: dir, maxBytes: limit)
        try spool.append(Data(repeating: 0, count: limit + 1))
        #expect(spool.segments.count == 2)
    }

    @Test func unknownSegmentThrows() throws {
        let spool = try Spool(directory: dir, maxBytes: 10)
        #expect(throws: SpoolError.self) { try spool.segment(named: "absent") }
    }
}
```

- Async XCTest: `func testX() async throws`; wait for expectations with `await fulfillment(of: [exp], timeout: 1)`. `wait(for:)` is marked `noasync`: it blocks the thread an async test runs on.
- Inject clocks (`any Clock<Duration>`) and file-system roots instead of sleeping or touching real paths.
- Run one test: `swift test --filter 'SpoolTests.SpoolWriterTests/testAppendsOneRecord'`; skip with `--skip`.

## Loom Test Runner Adapter

**Adapter.** `swift-test`. A directory containing `Package.swift` is a `swift` package, and loom's detection always picks `swift-test` for it. `loom project detect` prints the kind, runner and skill per package. An Xcode-only project without `Package.swift` is not detected as `swift`.

**Single-test command.** Loom runs each contract with the package directory as the working directory:

```bash
swift test --filter '{test}'
```

**The `test` field.** `Module.Class/testMethod`, where `Module` is the test target's module name. `swift test list` prints ids in this form. Example: `SpoolTests.SpoolWriterContractTests/testRejectsSymlinkedSpool`.

**No-match behaviour.** Documented: swift was not installed on the host that captured runner fixtures, so loom's parser for `swift-test` was written from the runner's documented output. Do not rely on the exit code. Loom reads the XCTest summary `Executed N tests, with F failures`; `Executed 0 tests` means nothing was selected, so a contract test whose name does not match fails the freeze. A plan criterion that runs `swift test --filter ...` and executes zero tests fails the same way (`selected zero tests (swift-test)`).

**Writing contract tests.**

- SwiftPM compiles every `.swift` file under `Tests/<TestTarget>/` into that test target. Put a contract in the existing test target, in a file named for its subject: `Tests/SpoolTests/SpoolWriterContractTests.swift`.
- Write contract tests as XCTest methods. The adapter's `test` format and its parser are XCTest's; a Swift Testing `@Test` function prints a separate summary that the adapter does not read, so a filter that selects only Swift Testing tests can read as zero tests executed.
- `--filter` is an unanchored regular expression. `testRejectsSymlink` also selects `testRejectsSymlinkedSpool`, so choose a method name that no other test id in the package contains.
- Any other file the contract session changes (a shared fixture, a new test target in `Package.swift`) must match a `harness` glob, or the freeze rejects it.
- Resolve and build once (`swift build --build-tests`) before writing contracts, so the freeze run compiles only your test target instead of fetching and building dependencies inside its 300-second limit. `.build/` must be in `.gitignore` (`swift package init` writes it); the freeze treats every other untracked path as a change.
- Loom cannot batch-select Swift tests by file, so impact-selected tests are skipped with a note for Swift packages; the full suite still runs in integration-verify.

```swift
import Foundation
import XCTest
@testable import Spool

final class SpoolWriterContractTests: XCTestCase {
    func testRejectsSymlinkedSpool() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }

        let target = dir.appendingPathComponent("target")
        let link = dir.appendingPathComponent("spool")
        try Data().write(to: target)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)

        XCTAssertThrowsError(try SpoolWriter(path: link.path)) { error in
            XCTAssertEqual(error as? SpoolError, .symlinked(path: link.path))
        }
        XCTAssertEqual(try Data(contentsOf: target), Data())
    }
}
```

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: Tests/SpoolTests/SpoolWriterContractTests.swift
    test: SpoolTests.SpoolWriterContractTests/testRejectsSymlinkedSpool
    scenario: the spool path is a symlink to an empty file in a fresh temporary directory
    rejects: a SpoolWriter that opens the path with default flags, follows the link and writes through it
```

**Build failures.** Swift is compiled: a contract test that does not compile yet (it calls an API the stage has not written) counts as red at freeze time; compiler `error:` lines mark the run as a build failure. One file that fails to compile fails the whole test target, so every test in that target stays red until the implementation compiles.

## Anti-Patterns

```swift
// Force unwrap on data from outside the program
let port = Int(env["PORT"]!)!                                   // BAD: crashes on a missing or bad value
guard let raw = env["PORT"], let port = Int(raw) else {         // GOOD
    throw ConfigError.missing(key: "PORT")
}

// Fire-and-forget task: its error vanishes and nothing can cancel it
Task { try await sync() }                                       // BAD
syncTask = Task { try await sync() }                            // GOOD: stored, cancelled in deinit/onDisappear

// Blocking a cooperative-pool thread to wait for async work
let done = DispatchSemaphore(value: 0)                          // BAD: can starve the pool and deadlock
Task { await work(); done.signal() }
done.wait()

// Retain cycle: self owns the closure, the closure owns self
timer.handler = { self.tick() }                                 // BAD
timer.handler = { [weak self] in self?.tick() }                 // GOOD
```

Quick swaps: `@unchecked Sendable` to silence a diagnostic → fix the type or guard it with a lock and say which; `DispatchQueue.main.async` inside async code → `@MainActor` isolation; `class` with no identity → `struct`; implicitly unwrapped optionals outside XCTest fixtures and IB outlets → a non-optional set in `init`; `catch { }` → handle, rethrow, or log with context; stringly-typed errors → an `Error` enum.

## Expert Practices

### Language Gotchas

- **`String` is a collection of grapheme clusters.** `count` is O(n), `"e\u{301}".count == 1` while `.utf8.count == 3`, and subscripts take a `String.Index` from the same string. Work on `.utf8` for byte protocols.
- **Integer overflow traps in release builds too.** `Int.max + 1` crashes. Use `&+`/`&*` for intended wrapping and `addingReportingOverflow` on untrusted sizes. `Int(someDouble)` traps on NaN, infinity and out-of-range values; use `Int(exactly:)`.
- **`lazy var` is not thread-safe.** Two threads can both run the initializer. A `static let` is lazily initialized exactly once, safely.
- **`defer` runs at scope exit in reverse order,** once per loop iteration when declared in a loop body.
- **`weak` becomes `nil`; `unowned` crashes** when the object is gone. Use `unowned` only when the lifetime relation is guaranteed.
- **A custom `==` needs a matching `hash(into:)`.** Synthesized conformances cover every stored property; override both or neither.

### Concurrency Gotchas

- **Global and static mutable state is an error in Swift 6 mode.** Make it a `let`, move it into an actor, or guard it with a `Mutex`. `nonisolated(unsafe) var` is a last resort that needs a comment explaining why no race is possible.
- **`MainActor.assumeIsolated { }`** asserts at runtime that a callback runs on the main thread; it traps otherwise. Prefer making the callback API `@MainActor`.
- **`AsyncStream` buffers without bound by default.** Pass `bufferingPolicy: .bufferingNewest(n)` for fast producers, and release resources in `continuation.onTermination`.
- **`Task.detached` is rarely right.** It drops priority, task-local values and actor context; a `nonisolated` async function or a `@concurrent` one (6.2) usually expresses the intent.
- **Actor-isolated state read before an `await` may be stale after it.** Write the "check then act" sequence without a suspension point in between, or record the in-flight work (see `DownloadCache` above).

### Codable

- **`JSONDecoder`'s default date strategy is `.deferredToDate`:** a `Double` of seconds since 2001-01-01. ISO 8601 strings need `.iso8601`, and that strategy rejects fractional seconds; decode those with `.custom` and `Date.ISO8601FormatStyle(includingFractionalSeconds: true)`.
- **Synthesized decoding ignores property defaults.** `var retries = 3` still throws `keyNotFound` when the key is absent; a `let` with an initial value is never decoded at all (the compiler warns). Make the field optional or write `init(from:)` with `decodeIfPresent(...) ?? 3`.
- **An unknown raw value throws `dataCorrupted`.** For enums the server may extend, decode through a type with an `unknown(String)` case.
- `keyDecodingStrategy = .convertFromSnakeCase` saves hand-written `CodingKeys` for snake-case payloads.

### Performance

- `final` classes and `private` members let the compiler devirtualize calls; release builds use whole-module optimization.
- Generic and `some` code specializes; `any` boxes values and dispatches through witness tables. Keep `any` out of hot loops.
- `reserveCapacity` before appending a known count; iterate `.utf8` or `.unicodeScalars` for byte-level parsing.
- `@inlinable` lets client modules specialize a generic function, and publishes its body as part of the module's interface. Use it on small, stable functions.
- Profile before tuning: Instruments on Apple platforms, `perf` on Linux, the package-benchmark plugin for regression tracking.

### Security

- **Open without following links.** A symlink check followed by a separate `open` races with an attacker swapping the path. Pass `O_NOFOLLOW` to `open(2)`; it fails with `ELOOP` on a symlink.

```swift
import Foundation   // re-exports Darwin or Glibc

func openSpool(atPath path: String) throws(SpoolError) -> Int32 {
    let fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_NOFOLLOW, 0o600)
    guard fd >= 0 else {
        if errno == ELOOP { throw .symlinked(path: path) }
        throw .notWritable(path: path)
    }
    return fd
}
```

- **Processes:** set `Process.executableURL` and pass an `arguments` array. Never build a `/bin/sh -c` string from input.
- **Randomness and crypto:** `SystemRandomNumberGenerator` (the default for `Int.random(in:)`) draws from the platform's secure source. Use CryptoKit or swift-crypto for hashes, HMAC and AEAD, and verify MACs with `HMAC<SHA256>.isValidAuthenticationCode(_:authenticating:using:)`, which compares in constant time.
- **Secrets** belong in the Keychain on Apple platforms. `UserDefaults` is a plain property list on disk.
- **Format strings:** `String(format:)` and `NSPredicate(format:)` with user-controlled format text are injection points. Keep the format literal and pass values as arguments.
- **TLS:** never answer a `URLSession` authentication challenge by trusting every certificate; pin with the server trust APIs when the threat model needs it.

## Verification Checklists

**Before marking Swift work done:**

- [ ] `swift format lint --strict` clean (or SwiftLint where the project configures it)
- [ ] `swift build --build-tests` with zero warnings in your own targets
- [ ] `swift test` green; new behaviour has tests, parameterized where inputs vary
- [ ] No `!`, `try!` or `as!` on data from outside the program; no `assert` used for validation
- [ ] Public API uses untyped `throws`; typed throws only on closed, internal error sets
- [ ] Dependencies added with `swift package add-dependency`; `Package.resolved` committed for applications
- [ ] `Codable` models tested against real payloads (dates, missing keys, unknown enum values)
- [ ] No shell strings built from input; secrets not stored in `UserDefaults`

**Concurrency review:**

- [ ] Swift 6 language mode, or strict concurrency enabled on targets still in Swift 5 mode
- [ ] Every `@unchecked Sendable` and `nonisolated(unsafe)` names the lock or invariant that makes it safe
- [ ] Every unstructured `Task` has a stored handle and a cancellation path
- [ ] Actor methods re-check state after each `await`; no check-then-act across a suspension
- [ ] Continuations resume exactly once on every path; no semaphores waiting on async work

**Loom contracts:**

- [ ] Contract tests are XCTest methods in the package's test target
- [ ] Each `test` value is the `Module.Class/testMethod` id from `swift test list`, and no other test id contains it
- [ ] Dependencies resolved and built before the freeze; each contract fails now, by assertion or by not compiling
