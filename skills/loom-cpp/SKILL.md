---
name: loom-cpp
description: C++ language expertise for idiomatic, production-quality code.
triggers:
  - cpp
  - "c++"
  - cmake
  - ctest
  - googletest
  - gtest
  - catch2
  - vcpkg
  - conan
  - clang-tidy
  - clang-format
  - raii
  - unique_ptr
  - shared_ptr
  - move semantics
  - "std::span"
  - string_view
  - concepts
  - constexpr
  - undefined behavior
  - undefined behaviour
  - sanitizer
  - asan
  - ubsan
---

# C++ Language Expertise

## Overview

Idiomatic, production-quality C++20/23 for an engineer who already knows the language. The expensive C++ mistakes compile cleanly: dangling views and references, undefined behaviour the optimizer exploits, resources nobody owns, and CMake targets that leak flags into each other. This skill is decision rules and traps. Check a compiler's support table (cppreference "compiler support") before relying on a C++23 library feature.

## Tooling

CMake describes the build, Ninja or Make runs it, CTest runs the tests. Keep the repository's generator, presets and package manager.

| Task | Command |
| --- | --- |
| Configure (out of source) | `cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON` |
| Configure from a preset | `cmake --preset debug` (`CMakePresets.json`, checked in) |
| Build | `cmake --build build -j` |
| Test | `ctest --test-dir build --output-on-failure -j 8` (`--test-dir` needs CMake 3.20+) |
| List registered tests | `ctest --test-dir build -N` |
| Format | `clang-format -i <files>`; CI: `clang-format --dry-run --Werror <files>` |
| Lint | `run-clang-tidy -p build` (reads `compile_commands.json` and `.clang-tidy`) |
| Add a dependency | `vcpkg add port fmt` (manifest mode, `vcpkg.json`); Conan 2: the recipe plus `conan install . --build=missing` |

- vcpkg (toolchain `-DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake`) and Conan 2 (`CMakeToolchain` generator) both hand CMake `find_package(... CONFIG REQUIRED)` targets. `FetchContent` downloads sources at configure time and needs network access then, which sandboxed sessions may lack; fetch test frameworks through the package manager or the system.
- clang-tidy: enable `bugprone-*`, `modernize-*`, `performance-*`, a pruned `cppcoreguidelines-*`, `misc-const-correctness`; set `WarningsAsErrors: '*'` in `.clang-tidy` so findings fail the gate. It must see the build's flags, which `compile_commands.json` provides.
- A pip-installed `cmake` shim earlier on `PATH` can shadow the real CMake suite (its `ctest` fails with `ModuleNotFoundError`). `command -v ctest` shows which binary runs.
- Multi-config generators (Visual Studio, Ninja Multi-Config) ignore `CMAKE_BUILD_TYPE`: pass `--config Debug` to `cmake --build` and `-C Debug` to `ctest`.

Target-based CMake: every setting belongs to a target, scoped `PRIVATE` (this target's build), `PUBLIC` (this target and its consumers) or `INTERFACE` (consumers only).

```cmake
cmake_minimum_required(VERSION 3.25)
project(spool LANGUAGES CXX)

add_library(spool src/spool_writer.cpp src/journal.cpp)
target_include_directories(spool PUBLIC include)
target_compile_features(spool PUBLIC cxx_std_20)
target_compile_options(spool PRIVATE
  "$<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-Wall;-Wextra;-Wpedantic;-Wconversion;-Wshadow;-Werror>"
  "$<$<CXX_COMPILER_ID:MSVC>:/W4;/WX;/permissive->")

include(CTest)                        # enable_testing() plus the BUILD_TESTING option
if(BUILD_TESTING)
  find_package(GTest CONFIG REQUIRED)
  add_executable(spool_tests tests/spool_writer_test.cpp)
  target_link_libraries(spool_tests PRIVATE spool GTest::gtest_main)
  include(GoogleTest)
  gtest_discover_tests(spool_tests)   # one CTest test per TEST(), named Suite.Name
endif()
```

- A generator expression holding several flags is quoted and `;`-separated. Unquoted spaces split it into broken arguments.
- `-Werror` goes on your own targets. A global `CMAKE_CXX_FLAGS` edit also applies it to third-party code built in the tree and breaks on the next compiler upgrade.

The gate:

```bash
clang-format --dry-run --Werror $(git ls-files '*.cpp' '*.hpp') \
  && cmake --build build \
  && run-clang-tidy -p build -quiet \
  && ctest --test-dir build --output-on-failure
```

## Modern C++ (20/23)

- **Concepts** replace SFINAE: constraints read as requirements and produce short errors.

```cpp
template <typename T>
concept Journal = requires(T j, std::span<const std::byte> bytes) {
    { j.append(bytes) } -> std::same_as<std::size_t>;
    j.flush();
};

void drain(Journal auto& journal, std::span<const Record> records);
```

- `auto operator<=>(const T&) const = default;` generates all six comparisons memberwise; a defaulted `operator==` alone gives `==` and `!=`.
- `std::format` (C++20) and `std::print`/`std::println` (C++23) check format strings at compile time; they replace `printf` and `iostream` chains.
- `std::expected<T, E>` (C++23) carries recoverable errors in the return value; `std::optional`'s `and_then`/`transform`/`or_else` (C++23) chain without nested `if`s.
- Ranges and views are lazy and composable: `records | std::views::filter(is_live) | std::views::transform(&Record::id)`. Views refer to their source (see Lifetimes). `filter_view` caches `begin()`, so it cannot be iterated through a `const&`, and modifying an element so it stops satisfying the predicate during iteration is undefined.
- Designated initializers `Options{.root = dir, .max_bytes = 1 << 20}` (declaration order only); `consteval` functions and `constexpr` containers move checks to compile time; `std::bit_cast`, `std::to_underlying` (C++23), `std::unreachable` (C++23).
- Deducing `this` (C++23 explicit object parameter) removes const/non-const overload pairs and CRTP boilerplate.
- Modules build with CMake 3.28+ and Ninja, and `import std;` is still experimental in CMake; headers remain the portable choice for libraries.
- Guard optional library features with feature-test macros: `#if __cpp_lib_expected >= 202202L`.

## Ownership and RAII

Every resource (memory, file, socket, lock, OS handle) has exactly one owning object whose destructor releases it.

| Need | Use |
| --- | --- |
| Sole owner | a value member, or `std::unique_ptr<T>` from `std::make_unique` |
| Shared ownership, lifetime unknown until runtime | `std::shared_ptr<T>` from `std::make_shared` |
| Observe a `shared_ptr` without extending it; break cycles | `std::weak_ptr<T>` |
| Non-owning access | `T&`, `const T&`, `T*` (nullable), `std::span<T>`, `std::string_view` |
| C handle | `std::unique_ptr<Handle, Deleter>` |

```cpp
struct FileCloser {
    void operator()(std::FILE* f) const noexcept { std::fclose(f); }
};
using File = std::unique_ptr<std::FILE, FileCloser>;

File open_journal(const std::filesystem::path& p) {
    File f{std::fopen(p.string().c_str(), "ab")};
    if (!f) throw std::system_error(errno, std::generic_category(), p.string());
    return f;
}

class SpoolWriter {  // rule of zero: every member manages itself
public:
    explicit SpoolWriter(std::filesystem::path root) : root_(std::move(root)) {}
    void add_sink(std::unique_ptr<Sink> sink) { sinks_.push_back(std::move(sink)); }  // ownership moves in
private:
    std::filesystem::path root_;
    std::vector<std::unique_ptr<Sink>> sinks_;
};
```

- **Rule of zero:** compose from members that manage themselves and declare none of the five special members. A class that manages a resource directly declares all five (destructor, copy and move constructors, copy and move assignment) or `= delete`s the ones it cannot support.
- Parameters: read-only `const T&` (by value for cheap types, `std::string_view`, `std::span`); sink parameters by value, then `std::move` into place; in/out `T&`; ownership transfer `std::unique_ptr<T>` by value. A `shared_ptr` by value on every call costs two atomic operations and says nothing about ownership; pass `const T&` unless the callee keeps a copy.
- Mark move constructors and move assignment `noexcept`. `std::vector` reallocation copies elements whose move can throw (`std::move_if_noexcept`), so growth quietly becomes deep copies.
- A moved-from object is valid but unspecified: assign to it or destroy it, never read it.
- `std::move` on a `const` object selects the copy constructor without a diagnostic. `return std::move(local);` blocks copy elision (`-Wpessimizing-move`); return the local by name.
- `make_shared` puts the object and control block in one allocation, so a live `weak_ptr` keeps that memory allocated until the last weak reference is gone. `shared_from_this()` in a constructor throws `std::bad_weak_ptr`: no `shared_ptr` owns the object yet.
- Destructors are implicitly `noexcept`; an exception escaping one calls `std::terminate`.

## Lifetimes: `string_view`, `span` and Dangling References

`std::string_view` and `std::span` are a pointer and a length. They never own and never extend the lifetime of what they point at. All of these compile:

```cpp
std::string_view name = std::string("spool-") + id;  // BAD: the temporary string dies at the ;

std::string_view make_label() {
    std::string s = build();
    return s;                                         // BAD: view into a destroyed local
}

struct Entry { std::string_view key; };
Entry e{config.get("key")};                           // BAD if get() returns std::string by value

std::vector<int> v = load();
std::span<int> head = std::span(v).first(3);
v.push_back(4);                                       // may reallocate: head now dangles

for (auto& r : fetch_batch().records()) { }           // BAD before C++23: the batch dies before the loop body
```

- Members and anything that outlives the call hold owning types (`std::string`, `std::vector`). Views are for parameters and locals whose source visibly outlives them.
- `string_view::data()` is not null-terminated. Passing it to a C API that expects a C string reads past the end; build a `std::string` first.
- Invalidation: a `vector` or `string` insertion that reallocates invalidates every iterator, reference and span into it; `unordered_map` rehashing invalidates iterators while references stay valid; erase during iteration uses the returned iterator or `std::erase_if` (C++20).
- A lambda capturing by reference (`[&]`) that outlives its scope (stored callback, thread, coroutine) dangles; capture by value or move owned state in (`[state = std::move(state)]`).
- A coroutine parameter taken by reference dangles once the caller's argument is destroyed while the coroutine is suspended. Coroutines take parameters by value.
- C++23 (P2718) extends every temporary in a range-`for` initializer to the end of the loop; older standards and compilers still dangle, so bind the owner to a named variable first.
- Tooling: GCC 13+ `-Wdangling-reference`; Clang `-Wdangling`, `-Wdangling-gsl` and `[[clang::lifetimebound]]` on accessor parameters; AddressSanitizer catches the rest at runtime (`ASAN_OPTIONS=detect_stack_use_after_return=1`).

## Undefined Behaviour and Sanitizers

Undefined behaviour (UB) is a rule violation the compiler assumes never happens. Optimizers delete checks that could only fail through UB (`if (x + 1 < x)` for signed `x` folds to `false`), so UB surfaces far from its cause and often only in release builds.

Common sources:

- signed integer overflow; shifting by a negative count or by the type's width or more; `INT_MIN / -1`
- out-of-bounds access (`operator[]` does not check), use after free, double free, dereferencing null or an end iterator
- reading an uninitialized variable; a non-`void` function falling off its end
- data races: two threads, at least one writing, no synchronization
- type punning through a pointer cast (strict aliasing); use `std::memcpy` or `std::bit_cast`
- modifying a `const` object or a string literal; misaligned access through a cast pointer
- a loop with no observable side effects that never terminates

Sanitizers instrument the build and report UB at runtime. Run the test suite under them in CI:

| Sanitizer | Flags | Finds | Notes |
| --- | --- | --- | --- |
| AddressSanitizer | `-fsanitize=address -fno-omit-frame-pointer` | out-of-bounds, use after free, leaks (LeakSanitizer on Linux) | about 2x slower; combine with UBSan |
| UndefinedBehaviorSanitizer | `-fsanitize=undefined -fno-sanitize-recover=all` | overflow, bad shifts, null, misalignment, invalid `bool`/enum loads | without `-fno-sanitize-recover` it prints and continues, and the test still passes |
| ThreadSanitizer | `-fsanitize=thread` | data races, lock-order inversions | separate build; cannot combine with ASan |
| MemorySanitizer | `-fsanitize=memory` (Clang only) | uninitialized reads | every linked library, the standard library included, must be instrumented |

```cmake
option(SPOOL_SANITIZE "Build with AddressSanitizer and UBSan" OFF)
if(SPOOL_SANITIZE)  # before any add_library/add_executable: only later targets pick it up
  add_compile_options(-fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer)
  add_link_options(-fsanitize=address,undefined)
endif()
```

- `-D_GLIBCXX_ASSERTIONS` (libstdc++) or `-D_LIBCPP_HARDENING_MODE=_LIBCPP_HARDENING_MODE_FAST` (libc++ 18+) adds bounds checks to `operator[]`, `front()` and similar at small cost, cheap enough for many production builds. GCC 14's `-fhardened` bundles `_GLIBCXX_ASSERTIONS` with `_FORTIFY_SOURCE=3`, the stack protector and PIE.
- Fuzz parsers and decoders with libFuzzer (`-fsanitize=fuzzer,address`).
- Optimization level changes which UB shows. Run sanitizer builds at `-O1` or higher as well as at `-O0`.

## Concurrency

- `std::jthread` (C++20) joins in its destructor and carries a `std::stop_token`. Destroying a joinable `std::thread` calls `std::terminate`.
- `std::scoped_lock` locks several mutexes with deadlock-free ordering; `std::unique_lock` when a `condition_variable` needs one. Wait with a predicate, which handles spurious wakeups: `cv.wait(lock, [&] { return !queue.empty(); });`.
- `std::atomic<T>` defaults to `memory_order_seq_cst`. A weaker order (`acquire`, `release`, `relaxed`) needs a written ordering argument, checked under ThreadSanitizer.
- A `shared_ptr`'s reference count is thread-safe; the pointee and the `shared_ptr` object itself are not. Share a pointer that changes through `std::atomic<std::shared_ptr<T>>` (C++20).
- The future from `std::async(std::launch::async, f)` blocks in its destructor, so a discarded call (`std::async(std::launch::async, f);`) runs `f` synchronously.
- Function-local statics initialize once, thread-safely; they avoid the static initialization order problem across translation units.

## Error Handling

- Exceptions for failures the immediate caller cannot handle; `std::expected<T, E>` or an error code where failure is an ordinary outcome or the path must not throw. Pick one style per module boundary and document it.
- `[[nodiscard]]` on functions that return an error value, so a dropped result warns.
- `noexcept` is a promise enforced by `std::terminate`. Mark moves, swaps and destructors `noexcept`; leave it off functions that allocate unless terminating on `std::bad_alloc` is acceptable.
- Every mutating operation gives at least the basic guarantee (no leaks, invariants hold); copy-and-swap gives the strong guarantee when callers need all-or-nothing.
- No exception may cross a C ABI boundary or leave a thread function: catch at the `extern "C"` entry and at the top of the thread, then translate it or carry it across with `std::exception_ptr`.

## Testing

GoogleTest and Catch2 both register their cases with CTest; CTest runs each registered test as a process and reads its exit code.

```cpp
#include <gtest/gtest.h>
#include "spool/spool_writer.hpp"

class SpoolWriterTest : public ::testing::Test {
protected:
    std::filesystem::path dir_ = make_temp_dir();
    void TearDown() override { std::filesystem::remove_all(dir_); }
};

TEST_F(SpoolWriterTest, AppendsRecordToJournal) {
    SpoolWriter writer{dir_};
    writer.append("a");
    ASSERT_TRUE(std::filesystem::exists(dir_ / "journal"));
    EXPECT_EQ(read_lines(dir_ / "journal").size(), 1U);
}

TEST(SpoolId, RejectsEmptyId) {
    EXPECT_THROW(SpoolId::parse(""), std::invalid_argument);
}
```

- `ASSERT_*` returns from the current function on failure; `EXPECT_*` records the failure and continues. Inside a helper, `ASSERT_*` returns from the helper only and the test goes on; wrap the call in `ASSERT_NO_FATAL_FAILURE(helper())`.
- Suite and test names contain no `_` (GoogleTest builds class names as `Suite_Name_Test`, so a `_` in either part can make two tests collide). Use CamelCase for both.
- Parameterized tests (`TEST_P` + `INSTANTIATE_TEST_SUITE_P`) are named `Prefix/Suite.Name/Index`. `EXPECT_THAT` with gMock matchers (`ElementsAre`, `HasSubstr`) gives readable failures. Death tests (`EXPECT_DEATH`) check aborts in a child process.
- `gtest_discover_tests(target)` runs the built executable with `--gtest_list_tests` after each build and registers one CTest test per case (`TEST_PREFIX`/`TEST_SUFFIX` decorate the names). `DISCOVERY_MODE PRE_TEST` (CMake 3.18+) moves listing to `ctest` time, which cross-compiled or slow-starting binaries need. `gtest_add_tests` scans the sources instead and misses cases the scan cannot see, such as tests generated by your own macros.

Catch2 v3:

```cpp
#include <catch2/catch_test_macros.hpp>

TEST_CASE("spool id rejects an empty string", "[spool]") {
    REQUIRE_THROWS_AS(SpoolId::parse(""), std::invalid_argument);
}
```

```cmake
find_package(Catch2 3 REQUIRED)
add_executable(spool_catch_tests tests/spool_id_test.cpp)
target_link_libraries(spool_catch_tests PRIVATE spool Catch2::Catch2WithMain)
include(Catch)
catch_discover_tests(spool_catch_tests)  # one CTest test per TEST_CASE, named by its description
```

- `REQUIRE` ends the test case on failure, `CHECK` continues. Each `SECTION` re-runs its enclosing `TEST_CASE` from the top, so setup runs once per leaf section.
- `ctest` selection: `-R <regex>`/`-E <regex>` by name, `-L <label>` by label, `-j N` in parallel, `--rerun-failed`, `--timeout <seconds>`, `-N` to list without running.
- `ctest -j` runs test processes concurrently. Tests that share a temporary directory, port or file need unique paths or the `RESOURCE_LOCK` test property.

## Loom Test Runner Adapter

**Adapter:** `ctest`. Loom marks a package `cpp` when its directory holds a `CMakeLists.txt`, and every `cpp` package gets `ctest`. `loom project detect` prints it per package:

```text
engine  kinds=cpp  runner=ctest  skills=loom-cpp
```

**Single-test command**, run with the package directory as cwd:

```bash
cmake --build build && ctest --test-dir build -R '^{test}$' --output-on-failure
```

CTest never compiles: `add_test` runs an executable that is already built. Without the `cmake --build build &&` prefix loom adds, a run after an edit would execute the stale binary from the previous build and a compile error would never surface. With it, the build runs first and `&&` stops before `ctest` when the build fails. `cmake --build` does not configure, so the package directory needs a configured binary directory named `build` (`cmake -S . -B build`, or a preset whose `binaryDir` is `${sourceDir}/build`).

**The `test` field** is the `add_test` name, exactly as `ctest --test-dir build -N` lists it:

| Registration | CTest name |
| --- | --- |
| `gtest_discover_tests(spool_tests)` | `Suite.Name`, e.g. `SpoolWriter.RejectsSymlinkedSpool` (after any `TEST_PREFIX`) |
| `catch_discover_tests(spool_tests)` | the `TEST_CASE` description, e.g. `spool writer rejects a symlinked spool` |
| `add_test(NAME spool_rejects_symlink COMMAND ...)` | `spool_rejects_symlink` |

Loom anchors the value as a regular expression (`-R '^{test}$'`). The `.` in `Suite.Name` also matches any character, which is harmless; `(`, `)`, `[`, `]`, `+`, `*`, `?`, `|` and `\` change what matches, and a `'` breaks the shell quoting. Keep contract test names to letters, digits, `.`, `/`, `_` (outside GoogleTest names) and spaces. The command does not use the contract's `file`; loom uses it to find the owning package and to freeze the file.

**No-match behaviour:** when the regex selects nothing, `ctest` exits **0** and prints `No tests were found!!!` on stderr (captured from a real run, CMake 4.2.3). Loom reads ctest's summary (`100% tests passed, 0 tests failed out of 1`), so a contract test whose name does not match is classified as not selected and fails the freeze ("the runner did not select the test"). Completion fails the same way.

**Writing contract tests:**

- Put test sources under `tests/`, one file per subject (`tests/spool_writer_test.cpp`); loom's `cpp` language profile decides which paths count as test files.
- Loom runs the command in the package that owns the contract file. A `tests/CMakeLists.txt` (pulled in with `add_subdirectory(tests)`) makes `tests/` a package of its own, with no `build/` inside it. Register test targets from the top-level `CMakeLists.txt` or from a file it `include()`s (`include(tests/tests.cmake)`), so the owning package is the configured project root. Confirm with `loom project detect` and one manual run of the command from that directory.
- A new test source joins a test target (the `add_executable` list or `target_sources`). That CMake edit is harness: list the file in the stage's `harness` globs. `gtest_discover_tests` and `catch_discover_tests` pick up the new cases on the next build.
- Keep `build/` in `.gitignore`. The freeze rejects any changed or untracked path that is neither a contract `file` nor a `harness` match.
- GoogleTest: `TEST(SpoolWriter, RejectsSymlinkedSpool)` gives the CTest name `SpoolWriter.RejectsSymlinkedSpool`. Catch2: a unique `TEST_CASE` description free of regex metacharacters.
- Set `runner: ctest` on a contract only to override detection.

```yaml
harness:
  - tests/tests.cmake
contracts:
  - id: rejects-symlinked-spool
    file: tests/spool_writer_test.cpp
    test: SpoolWriter.RejectsSymlinkedSpool
    scenario: creates a spool directory whose journal path is a symlink to a file outside the spool root, then constructs SpoolWriter on the directory and appends one record
    rejects: a SpoolWriter that opens the journal with plain fopen, follows the symlink and appends to the file outside the spool root
```

**Build failures:** a contract test that calls a function or type the stage has not written yet fails `cmake --build` (the captured fixture exits 2 with compiler diagnostics on stderr); `&&` skips `ctest`, loom classifies the run as a build failure, and the freeze accepts that as red. A function that is declared but not defined fails at link time and counts the same way. `cmake --build build` builds every target, so an unrelated compile error anywhere in the project also reads as red: check that the diagnostics name the API the contract is waiting for.

## Anti-Patterns

```cpp
// Owning raw pointer: leaks on every early return and exception
Widget* w = new Widget(cfg); use(*w); delete w;           // BAD
auto w = std::make_unique<Widget>(cfg); use(*w);           // GOOD

// Pointer plus count: no bounds, easy to mismatch
void fill(int* buf, int n);                                // BAD
void fill(std::span<int> buf);                             // GOOD

// Brace initialization prefers initializer_list constructors
std::vector<int> a{3, 1};                                  // two elements: 3, 1
std::vector<int> b(3, 1);                                  // three elements: 1, 1, 1

// Unsigned countdown never ends (and v.size() - 1 wraps when v is empty)
for (std::size_t i = v.size() - 1; i >= 0; --i) { }        // BAD
for (std::size_t i = v.size(); i-- > 0;) { }               // GOOD

// map::operator[] inserts on lookup
if (counts[key] > 0) { }                                   // BAD: inserts key -> 0
if (auto it = counts.find(key); it != counts.end() && it->second > 0) { }  // GOOD
```

Quick swaps:

- `using namespace std;` in a header → qualified names (the directive leaks into every includer).
- `#define` constants and function-like macros → `constexpr` variables and functions.
- C-style casts → `static_cast` or `std::bit_cast`; `reinterpret_cast` only with a comment proving it valid.
- `std::endl` in a loop → `'\n'` (`endl` flushes every time).
- `strcpy`, `sprintf`, `atoi` → `std::string`, `std::format`, `std::from_chars`.
- Signed/unsigned comparison → `std::cmp_less` (C++20) or `std::ssize`.
- `catch (...) {}` → the specific exception types; `catch (...)` only at a boundary that logs or translates.
- Returning `const T` by value → plain `T` (a `const` return value blocks moves).
- `std::vector<bool>` as a buffer → `std::vector<char>` or `std::bitset` (`vector<bool>` is a bit-packed proxy with no `bool*`).
- Mutable globals and singletons → dependencies passed explicitly.

## Expert Practices

### Performance

- Measure first: `perf` or another sampling profiler on a `RelWithDebInfo` build; Google Benchmark with `benchmark::DoNotOptimize` for microbenchmarks.
- Memory layout dominates: a contiguous `std::vector` of values beats node containers; `reserve` when the size is known; `emplace_back` constructs in place. `std::unordered_map` allocates a node per element; open-addressing maps (`absl::flat_hash_map`, `boost::unordered_flat_map`) are faster for hot lookups.
- Range-`for` over non-trivial elements uses `const auto&`; plain `auto` copies each one.
- `std::string_view` and `std::span` parameters accept strings and buffers from any owner without a copy.
- Link-time optimization: `set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)` after `check_ipo_supported()` (from `include(CheckIPOSupported)`). `-march=native` only when the build machine is the deployment CPU.
- `[[likely]]`/`[[unlikely]]` only after a profile shows a mispredicted branch.

### Correctness Gotchas

- Most vexing parse: `Widget w();` declares a function; write `Widget w{};`.
- Give every scalar member a default member initializer (`int count_ = 0;`); `-Wuninitialized` and MemorySanitizer catch the rest.
- One Definition Rule: functions and variables defined in a header are `inline` (or templates, or `constexpr` functions); a `static` or anonymous-namespace entity in a header is duplicated in every translation unit.
- Static initialization order across translation units is unspecified; a global whose constructor reads another global may see it unconstructed. Use a function-local static.
- Integer promotion: `std::uint8_t + std::uint8_t` is `int`; `-Wconversion` reports the narrowing on assignment back.
- `std::unordered_map` iteration order is unspecified and changes on rehash; tests must not depend on it.
- There is no `std::optional<T&>` before C++26; an optional reference is a `T*`.

### Security

- Validate untrusted indices and sizes before use; hardened standard library modes turn silent overreads into aborts.
- Check size arithmetic (`count * sizeof(T)`, `offset + length`) for overflow before allocating or slicing (`__builtin_mul_overflow`, `std::numeric_limits`).
- Never pass untrusted text as a `printf` format. `std::format` requires a constant format string; `std::vformat` is the deliberate runtime escape hatch.
- Check-then-open (`std::filesystem::exists`, then `fopen`) races with an attacker who swaps in a symlink. Open with `O_NOFOLLOW` or `openat` relative to a trusted directory descriptor, then `fstat` the opened descriptor.
- `std::system` runs a shell; build an `argv` array and use `posix_spawn` or `execve`.
- Release hardening flags: `-D_FORTIFY_SOURCE=3` (needs optimization), `-fstack-protector-strong`, `-fstack-clash-protection`, PIE, `-Wl,-z,relro,-z,now`.
- Clear secrets with `explicit_bzero` or `SecureZeroMemory`; a plain `memset` right before `free` can be optimized away.

## Verification Checklists

**Before marking C++ work done:**

- [ ] `clang-format --dry-run --Werror` clean; `run-clang-tidy -p build` clean with `WarningsAsErrors` set
- [ ] Builds warning-free with `-Wall -Wextra -Wpedantic -Wconversion -Werror` (or `/W4 /WX`) on every supported compiler
- [ ] `ctest --output-on-failure` green, and green again in an ASan+UBSan build with `-fno-sanitize-recover=all`
- [ ] No owning raw pointers or bare `new`/`delete`; each class follows the rule of zero or declares all five special members
- [ ] Views (`string_view`, `span`, ranges) never outlive their source; no view stored in a member without a documented owner
- [ ] Moves are `noexcept`; no `std::move` on `const` objects or on returned locals
- [ ] Untrusted sizes and indices checked before arithmetic and access; no shell command strings built from input
- [ ] Settings attached to targets with `PRIVATE`/`PUBLIC`/`INTERFACE`; no global `CMAKE_CXX_FLAGS` edits
- [ ] Dependencies come from the package manager (`vcpkg add port`, the Conan recipe); `build/` is gitignored

**Concurrency review:**

- [ ] The concurrent tests pass under ThreadSanitizer
- [ ] Every shared mutable object is guarded by a mutex or is atomic; `condition_variable` waits use a predicate
- [ ] Threads are `std::jthread` or joined on every path; no discarded `std::async` futures
- [ ] Every non-`seq_cst` memory order carries a comment with its ordering argument
