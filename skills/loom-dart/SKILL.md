---
name: loom-dart
description: Dart language expertise for idiomatic, production-quality code.
triggers:
  - dart
  - flutter
  - pub
  - pubspec
  - pub.dev
  - dart test
  - flutter test
  - flutter_test
  - package:test
  - null safety
  - isolate
  - statefulwidget
  - statelesswidget
  - riverpod
  - provider
  - bloc
  - build_runner
  - freezed
  - json_serializable
  - mocktail
  - dart analyze
  - dart format
---

# Dart Language Expertise

## Overview

Idiomatic, production-grade Dart 3 and Flutter: pub packages, sound null safety, records and patterns, futures and streams, isolates, and the widget lifecycle. Assumes the reader knows the syntax. The value is in what is easy to get wrong: unawaited futures, public fields that never promote, `BuildContext` used after an `await`, futures created in `build`, isolate message copying, and a test filter that matches by substring.

## Tooling

`dart` (or `flutter` in a Flutter project) is the build tool, package manager, formatter, analyzer and test runner in one.

| Task | Command |
| --- | --- |
| New package | `dart create -t package spool` / `flutter create --template=package spool` / `flutter create app` |
| Add / remove dep | `dart pub add path` / `dart pub remove path` (`flutter pub add` in Flutter projects) |
| Add dev dep | `dart pub add dev:test` |
| Resolve | `dart pub get` (`--enforce-lockfile` in CI fails on drift from `pubspec.lock`) |
| Outdated / upgrade | `dart pub outdated` / `dart pub upgrade --major-versions` |
| Format | `dart format .` |
| Analyze | `dart analyze --fatal-infos` / `flutter analyze` |
| Apply fixes | `dart fix --apply` |
| Code generation | `dart run build_runner build --delete-conflicting-outputs` |
| Test | `dart test` / `flutter test` |
| Native binary | `dart compile exe bin/spool.dart -o spool` |

- Use `dart pub add` so `pubspec.yaml` and `pubspec.lock` stay in step. Commit `pubspec.lock` for applications.
- Monorepos (3.6+): a root `pubspec.yaml` lists `workspace: [packages/a, packages/b]` and each member declares `resolution: workspace`, giving one shared resolution and one lockfile.
- Generated files (`*.g.dart`, `*.freezed.dart`) come from `build_runner`; never edit them by hand.

```yaml
# analysis_options.yaml
include: package:lints/recommended.yaml   # Flutter: package:flutter_lints/flutter.yaml

analyzer:
  language:
    strict-casts: true        # no implicit downcast from dynamic
    strict-inference: true
    strict-raw-types: true
  errors:
    unawaited_futures: error

linter:
  rules:
    - unawaited_futures
    - discarded_futures
    - cancel_subscriptions
    - close_sinks
    - avoid_print
```

The canonical gate (swap in `flutter analyze` and `flutter test` for Flutter packages):

```bash
dart format --output=none --set-exit-if-changed .
dart analyze --fatal-infos
dart test
```

## Sound Null Safety

- Types are non-nullable unless marked `T?`, and the guarantee is sound: a non-nullable static type never holds `null` at runtime.
- Type promotion after a null check applies to local variables and parameters, and (3.2+) to private `final` fields. Public fields and getters never promote, because a subclass or another library could override them. Copy to a local first.
- `!` throws `Null check operator used on a null value` at runtime. Each `!` is an unchecked claim; prefer promotion, `??`, or an early return.
- `late` defers initialization. Reading a `late` field before it is written throws `LateInitializationError`. `late final x = compute();` runs `compute` lazily on first read.
- Mark mandatory named parameters `required`. A nullable parameter checked inside the function moves the error from compile time to runtime.

```dart
class Spool {
  Spool(this.path, {this.maxBytes});

  final String path;
  final int? maxBytes; // public: never promotes

  bool isFull(int written) {
    final limit = maxBytes; // a local promotes
    if (limit == null) return false;
    return written >= limit; // limit is int here
  }
}
```

## Types, Records and Patterns

- `sealed` classes make `switch` exhaustive: adding a subtype turns every non-exhaustive switch into a compile error.
- Class modifiers state intent: `final` (no subtypes outside the library), `base` (inherit only), `interface` (implement only), `mixin class`.
- Records return several values without a class: `(int, String)`, `({int line, int column})`. Records compare by value.
- Patterns validate and destructure untyped data such as decoded JSON in one step.
- Extension types (3.3) wrap a representation type at zero runtime cost, useful for ids and units.
- Override `==` and `hashCode` together, or use a record or a generated data class.

```dart
sealed class SpoolEvent {}

final class Appended extends SpoolEvent {
  Appended(this.bytes);
  final int bytes;
}

final class Rotated extends SpoolEvent {
  Rotated(this.segment);
  final String segment;
}

String describe(SpoolEvent event) => switch (event) {
      Appended(:final bytes) => 'appended $bytes bytes',
      Rotated(:final segment) => 'rotated to $segment',
    };

extension type SegmentId(int value) {}

Config parseConfig(Object? json) {
  if (json case {'path': String path, 'maxBytes': int maxBytes}) {
    return Config(path, maxBytes);
  }
  throw FormatException('invalid config', json);
}
```

## Errors

- `Exception` types describe expected failures that callers handle. `Error` types (`StateError`, `ArgumentError`, `RangeError`) describe programming bugs; do not catch them in normal flow.
- Catch by type with `on FormatException catch (e, st)`. A bare `catch (e)` also catches `Error`s; keep it to top-level handlers that log and report.
- `rethrow` keeps the original stack trace. When translating an error, use `Error.throwWithStackTrace(SpoolException(...), st)`.
- An error thrown inside an `async` function completes its `Future` with that error; nobody sees it unless the future is awaited or handled.
- In Flutter, route uncaught errors to reporting through `FlutterError.onError` and `PlatformDispatcher.instance.onError`.

```dart
final class SpoolException implements Exception {
  SpoolException(this.message, {this.path});
  final String message;
  final String? path;

  @override
  String toString() => 'SpoolException: $message${path == null ? '' : ' ($path)'}';
}

Future<Config> loadConfig(File file) async {
  try {
    return parseConfig(jsonDecode(await file.readAsString()));
  } on FormatException catch (e, st) {
    Error.throwWithStackTrace(SpoolException(e.message, path: file.path), st);
  }
}
```

## Async and Streams

- Await every `Future`, or hand it to `unawaited(...)` (from `dart:async`) so the intent is visible. The `unawaited_futures` and `discarded_futures` lints enforce this.
- `(a, b).wait` on a record of futures (3.0+) runs them concurrently and throws a `ParallelWaitError` carrying every result and error. `Future.wait` reports only the first error.
- `future.timeout(d)` stops waiting; the underlying work keeps running.
- Streams are single-subscription by default; a second `listen` throws `Bad state: Stream has already been listened to.` Use `StreamController.broadcast()` for multiple listeners.
- Cancel every `StreamSubscription` and close every `StreamController` you create (`cancel_subscriptions`, `close_sinks`).
- An `async*` generator pauses at `yield` while its subscriber is paused, which gives backpressure for free.

```dart
Future<Dashboard> load(UserId id) async {
  final (profile, orders) = await (api.profile(id), api.orders(id)).wait;
  return Dashboard(profile, orders);
}

Stream<Segment> segments(Directory dir) async* {
  await for (final entity in dir.list()) {
    if (entity is File && entity.path.endsWith('.seg')) {
      yield await Segment.open(entity);
    }
  }
}

final subscription = watcher.events.listen(onEvent, onError: onError);
// ...
await subscription.cancel(); // otherwise the listener and its resources stay alive
```

## Isolates

- Each isolate has its own heap and event loop; there is no shared mutable memory. Messages are copied, except `TransferableTypedData` and the value returned through `Isolate.exit`.
- `Isolate.run(() => work())` (2.19+) runs one computation on a new isolate and returns its result; use it for parsing or crunching that would stall the UI or a server's event loop.
- The closure sent to an isolate carries everything it captures. Capturing `this` copies the whole object graph, and an unsendable member (a `Socket`, a `ReceivePort`, a native handle) throws `Illegal argument in isolate message`. Capture only plain data.
- Long-lived workers: `Isolate.spawn` with a `SendPort`/`ReceivePort` pair. Flutter's `compute()` wraps `Isolate.run`; on the web there are no isolates and `compute()` runs on the main thread.

```dart
Future<List<Entry>> parseLarge(Uint8List bytes) =>
    Isolate.run(() => decodeEntries(bytes)); // bytes are copied into the new isolate
```

## Flutter Essentials

- Widgets are immutable configuration. `const` constructors let Flutter skip rebuilding identical subtrees.
- `setState` wraps a synchronous mutation only. Do async work first, then call `setState` with the result.
- After any `await` in a `State` method, check `mounted` (or `context.mounted` outside a `State`) before touching `context` or calling `setState`. The `use_build_context_synchronously` lint flags misses.
- Create futures and streams in `initState`, never in `build`: `build` runs often, and a new future restarts `FutureBuilder` each time.
- Dispose controllers, focus nodes, animation controllers and subscriptions in `dispose`.
- Give list items that can move a `ValueKey(item.id)` so their state follows the item.
- State management: `ValueNotifier`/`ChangeNotifier` with `ListenableBuilder` covers small apps. Follow the project's existing Provider, Riverpod or Bloc setup; do not mix them.
- Split large `build` methods into small widget classes; helper methods returning widgets rebuild with their parent and cannot be `const`.

```dart
class SpoolPage extends StatefulWidget {
  const SpoolPage({super.key, required this.client});
  final SpoolClient client;

  @override
  State<SpoolPage> createState() => _SpoolPageState();
}

class _SpoolPageState extends State<SpoolPage> {
  late final Future<List<String>> _entries;

  @override
  void initState() {
    super.initState();
    _entries = widget.client.list(); // runs once per State
  }

  Future<void> _purge() async {
    await widget.client.purge();
    if (!mounted) return; // the page may be gone after the await
    ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Purged')));
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<String>>(
      future: _entries,
      builder: (context, snapshot) {
        if (snapshot.hasError) return Text('Error: ${snapshot.error}');
        final entries = snapshot.data;
        if (entries == null) return const CircularProgressIndicator();
        return ListView.builder(
          itemCount: entries.length,
          itemBuilder: (context, i) => ListTile(title: Text(entries[i]), onTap: _purge),
        );
      },
    );
  }
}
```

## Testing

`package:test` provides `test`, `group`, `setUp`, `tearDown`, `addTearDown` and the matcher library; `flutter_test` re-exports it and adds `testWidgets` with a `WidgetTester`.

```dart
import 'package:mocktail/mocktail.dart';
import 'package:spool/spool.dart';
import 'package:test/test.dart';

class MockClient extends Mock implements SpoolClient {}

void main() {
  group('Spool', () {
    late MockClient client;

    setUp(() => client = MockClient());

    test('rotates when the limit is reached', () async {
      when(() => client.rotate()).thenAnswer((_) async {});
      final spool = Spool(client, maxBytes: 4);
      await spool.append([1, 2, 3, 4, 5]);
      expect(spool.segments, hasLength(2));
      verify(() => client.rotate()).called(1);
    });

    test('rejects an empty path', () {
      expect(() => Spool.open(''), throwsA(isA<ArgumentError>()));
    });

    test('emits appended then rotated', () async {
      when(() => client.rotate()).thenAnswer((_) async {});
      final spool = Spool(client, maxBytes: 1);
      final events = expectLater(spool.events, emitsInOrder([isA<Appended>(), isA<Rotated>()]));
      await spool.append([1, 2]);
      await events;
    });
  });
}
```

```dart
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('purge shows a confirmation', (tester) async {
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: SpoolPage(client: FakeClient()))));
    await tester.pump(); // resolve the entries future
    await tester.tap(find.text('first'));
    await tester.pump();
    expect(find.text('Purged'), findsOneWidget);
  });
}
```

- Async tests return their `Future`; the runner waits for it. `expectLater` returns a future to await when the assertion itself is asynchronous.
- `pumpAndSettle` pumps until no frames are scheduled and times out on endless animations such as a spinning `CircularProgressIndicator`; use `pump(duration)` there.
- Control time with `package:fake_async` or an injected `Clock` from `package:clock`.
- Golden tests (`matchesGoldenFile`, `flutter test --update-goldens`) render fonts differently across operating systems; generate and compare them on one platform.
- Useful flags: `--name` (regex), `--plain-name` (substring), `-t`/`-x` for tags, `-p chrome`, `--reporter expanded`, `--coverage=coverage` (`--coverage` for Flutter).

## Loom Test Runner Adapter

**Adapters.** `dart-test` and `flutter-test`. A directory containing `pubspec.yaml` is a `dart` package; loom picks `flutter-test` when `pubspec.yaml` depends on `flutter`, else `dart-test`. `loom project detect` prints the kind, runner and skill per package; confirm that a pure Dart package reports `dart-test`, since `flutter test` needs the Flutter SDK.

**Single-test command.** Loom runs each contract with the package directory as the working directory:

```bash
dart test {file} --plain-name '{test}'
flutter test {file} --plain-name '{test}'
```

**The `test` field.** The test description. `--plain-name` matches it as a plain substring of each test's full name, which is the enclosing `group` descriptions and the test description joined by spaces. Example: for `test('refuses a spool path that is a symlink', ...)` inside `group('SpoolWriter', ...)`, both `refuses a spool path that is a symlink` and `SpoolWriter refuses a spool path that is a symlink` select it.

**No-match behaviour.** Both runners exit non-zero when nothing matches, as captured in loom's runner fixtures: `dart test` exits 79 and `flutter test` exits 1, each printing `No tests match "<name>".` on stderr and `No tests ran.` on stdout. Loom reads the runner's summary, so a contract test whose name does not match reads as not selected and fails the freeze. `dart test` colours its output even through a pipe; loom strips the colour codes before parsing, so `--no-color` is unnecessary.

**Writing contract tests.**

- A plain `dart test` or `flutter test` runs every file ending in `_test.dart` under `test/`. Put contract tests there, named for their subject: `test/spool_writer_contract_test.dart`. A file without the `_test.dart` suffix is skipped by full runs.
- Give each contract test a description that no other test's full name in the file contains, or put the full group-prefixed name in `test`; a substring shared by two tests selects both.
- Keep apostrophes out of contract descriptions: the value is substituted into a single-quoted shell argument.
- Run `dart pub get` (or `flutter pub get`) before writing contracts, so the freeze run does not resolve packages inside its 300-second limit. `.dart_tool/` must be in `.gitignore`; the freeze treats every other untracked path as a change, and any extra file you touch (a shared fixture under `test/fixtures/`) must match a `harness` glob.
- For impact-selected tests loom batches files into one command (`dart test f1 f2`, `flutter test f1 f2`).

```dart
import 'dart:io';

import 'package:spool/spool.dart';
import 'package:test/test.dart';

void main() {
  group('SpoolWriter', () {
    late Directory dir;

    setUp(() async {
      dir = await Directory.systemTemp.createTemp('spool_contract');
      addTearDown(() => dir.delete(recursive: true));
    });

    test('refuses a spool path that is a symlink', () async {
      final target = File('${dir.path}/target')..writeAsStringSync('');
      final link = Link('${dir.path}/spool');
      await link.create(target.path);

      await expectLater(SpoolWriter.open(link.path), throwsA(isA<SpoolSymlinkException>()));
      expect(target.readAsStringSync(), isEmpty);
    });
  });
}
```

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: test/spool_writer_contract_test.dart
    test: SpoolWriter refuses a spool path that is a symlink
    scenario: the spool path is a symlink to an empty file in a fresh temporary directory
    rejects: a SpoolWriter.open that follows the link and appends to the target file
```

**Build failures.** Dart compiles each test file before running it. A contract test that does not compile yet (it calls an API the stage has not written) fails to load with `Failed to load "<file>"` and compiler errors; loom reads that as a build failure, which counts as red at freeze time. Only that file fails to load; other test files still run.

## Anti-Patterns

```dart
// Fire-and-forget future: its error is uncaught and ordering is lost
void save() { repository.write(entry); }                 // BAD
Future<void> save() => repository.write(entry);          // GOOD: the caller awaits

// forEach with an async callback does not wait for anything
items.forEach((item) async => await upload(item));       // BAD: all start, none awaited
for (final item in items) { await upload(item); }        // GOOD (or Future.wait for concurrency)

// Trusting decoded JSON with casts
final port = (jsonDecode(body) as Map)['port'] as int;    // BAD: TypeError on bad input
if (jsonDecode(body) case {'port': int port}) { /* use port */ }   // GOOD: validated

// BuildContext after an await
await client.purge();
Navigator.of(context).pop();                             // BAD: the widget may be unmounted
if (context.mounted) Navigator.of(context).pop();        // GOOD
```

Quick swaps: `late` added only to silence the analyzer → initialize in the constructor or make it nullable; `dynamic` parameters → a real type or `Object?`; `print` in library code → `package:logging`; catching `Error` → fix the bug; a `BuildContext` stored in a field → pass it per call; mutable global singletons → inject dependencies through constructors.

## Expert Practices

### Language Gotchas

- **Collections compare by identity.** `[1] == [1]` is `false`. Use `ListEquality` from `package:collection`, records (value equality), or a data class. `const` collections are canonicalized, so `identical(const [1], const [1])` is `true`.
- **`int` differs by platform.** Native `int` is 64-bit and wraps on overflow; on the web it is a JavaScript number with 53 bits of integer precision. IDs and hashes beyond 2^53 need `BigInt` or a string on the web.
- **`Iterable` operations are lazy.** `map` and `where` rerun their callbacks on every iteration; call `.toList()` once when the result is used more than once or the callback has side effects.
- **Sync errors inside an `async` function become future errors.** A caller that does not await never sees them.
- **`DateTime.now()` is local time.** Store and compare in UTC (`toUtc()`), and inject a clock in code under test.

### Async and Isolate Gotchas

- **Unhandled future errors are reported to the zone.** In a CLI they terminate the isolate; in Flutter they reach `PlatformDispatcher.instance.onError`. Keep `unawaited_futures` on.
- **Microtasks run before the next event.** A loop that keeps scheduling microtasks (or a chain of already-completed futures) starves timers and I/O.
- **Isolate startup is not free.** Spawning per small task costs more than the work; batch the input or keep a long-lived worker isolate.
- **`StreamController` without `onCancel`** keeps producing after the listener leaves. Stop the producer in `onCancel`.

### Performance

- `const` widgets and constructors; `ListView.builder` for long or unbounded lists.
- Animate with `FadeTransition` and similar widgets; animating an `Opacity` rebuilds and repaints its subtree each frame.
- Wrap frequently repainting regions in `RepaintBoundary`, and verify with the DevTools performance overlay.
- Move JSON decoding of large payloads and other CPU work to `Isolate.run`.
- Build strings with `StringBuffer` in loops. Ship CLIs as `dart compile exe` AOT binaries for fast startup.

### Security

- **Randomness:** `Random.secure()` for tokens, nonces and keys; `Random()` is predictable.
- **Processes:** `Process.run(executable, args)` passes arguments directly. `runInShell: true` with interpolated input is shell injection.
- **Paths:** normalize with `package:path` and check `p.isWithin(root, candidate)`. Symlinks defeat that check, so compare `File(candidate).resolveSymbolicLinksSync()` against the root or refuse links (`FileSystemEntity.isLinkSync`).
- **TLS:** never return `true` from `HttpClient.badCertificateCallback` outside a test.
- **App secrets:** values passed with `--dart-define` are compiled into the binary and can be extracted. Keep server secrets on the server. Store user tokens in platform secure storage; `SharedPreferences` is plain text on disk.

## Verification Checklists

**Before marking Dart work done:**

- [ ] `dart format --output=none --set-exit-if-changed .` clean
- [ ] `dart analyze --fatal-infos` (or `flutter analyze`) clean with strict casts, inference and raw types on
- [ ] `dart test` / `flutter test` green; new behaviour has tests
- [ ] No `!` on values from outside the program; decoded JSON validated with patterns or typed models
- [ ] Every future awaited or wrapped in `unawaited`; every subscription cancelled and controller closed
- [ ] Dependencies added with `dart pub add`; generated files regenerated with `build_runner`
- [ ] No secrets in `--dart-define` or source; `Random.secure()` for anything security-related

**Flutter review:**

- [ ] No futures or streams created in `build`; controllers disposed in `dispose`
- [ ] `mounted` / `context.mounted` checked after every `await` before using `context` or `setState`
- [ ] `const` constructors used where possible; reorderable list items keyed by identity
- [ ] Heavy work moved off the UI isolate; the project's state-management approach followed

**Loom contracts:**

- [ ] Contract files live under `test/` and end in `_test.dart`
- [ ] Each `test` value is a description or full group-prefixed name that selects exactly one test, with no apostrophes
- [ ] Packages resolved before the freeze; each contract fails now, by assertion or by failing to load
