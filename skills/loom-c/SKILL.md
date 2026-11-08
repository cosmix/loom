---
name: loom-c
description: C language expertise (C11/C17/C23) for memory-safe, portable, production-quality systems code.
triggers:
  - c language
  - c11
  - c17
  - c23
  - ansi c
  - posix c
  - libc
  - malloc
  - realloc
  - snprintf
  - errno
  - header file
  - undefined behavior
  - valgrind
  - asan
  - ubsan
  - clang-tidy
  - cppcheck
---

# C Language Expertise

## Overview

Idiomatic, defensive C17 (C23 where the toolchain supports it): who owns each allocation, how failure is reported, which operations are undefined behaviour (UB), how buffers are bounded, and how the build, sanitizers and tests catch what the compiler cannot. Assumes the reader knows the language. The content is decision rules and the traps that survive review: `malloc(n * size)` overflow, `realloc` assigned back to its only pointer, `strncpy` leaving no terminator, signed overflow in a bounds check, a `char` passed to `isalpha`, a return value ignored.

## Tooling

| Task | Command |
| --- | --- |
| Configure (CMake) | `cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug` |
| Build | `cmake --build build` |
| Test, whole suite (the stage gate's) | `ctest --test-dir build --output-on-failure` |
| One test | `ctest --test-dir build -R '^spool_rejects_symlink$' --output-on-failure` |
| Static analysis | `clang-tidy -p build src/*.c`, `cppcheck --enable=warning,style,performance --error-exitcode=1 src/` |
| Format | `clang-format -i src/*.c include/*.h` |
| Memory check | `valgrind --leak-check=full --error-exitcode=1 ./build/tests/spool_test` |

Keep the build system the repository already has. A new project uses CMake (3.20+) with presets, which also feeds `clang-tidy` through `compile_commands.json`.

```cmake
cmake_minimum_required(VERSION 3.20)
project(spool LANGUAGES C)

set(CMAKE_C_STANDARD 17)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

add_library(spool src/spool.c)
target_include_directories(spool PUBLIC include)
target_compile_options(spool PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wconversion -Wformat=2)

enable_testing()
add_executable(spool_test tests/spool_test.c)
target_link_libraries(spool_test PRIVATE spool)
add_test(NAME spool_appends_record COMMAND spool_test)
```

- **Warnings:** `-Wall -Wextra -Wpedantic -Wshadow -Wconversion -Wformat=2`, and `-Werror` in CI only (a new compiler release adds warnings and should not break a downstream build). `-Wconversion` is noisy at first and finds signed/unsigned and narrowing bugs.
- **Standard:** pin `-std=c17` (or `c11`) and turn GNU extensions off (`CMAKE_C_EXTENSIONS OFF`) unless the code needs them. C23 adds `nullptr`, `bool`/`true`/`false` as keywords, `constexpr`, `typeof`, `#embed`, `[[nodiscard]]` and `<stdckdint.h>`; check the minimum GCC (13+) and Clang (18+) before relying on it.
- **Debug builds:** `-O0 -g3`; sanitized builds use `-O1 -g -fno-omit-frame-pointer`. Optimised builds are where UB bites, so run the tests under `-O2` too.

## Memory Ownership

Every heap block has exactly one owner, which frees it exactly once. State the owner in the function's comment and name: `*_new`/`*_free` pairs, `*_take` for a function that consumes its argument, `*_borrow` or `const T *` for a view.

```c
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *name;
    int *values;
    size_t len;
} Series;

void series_free(Series *s);

/* Returns NULL on allocation failure; the caller owns the result and calls series_free. */
Series *series_new(const char *name, size_t len) {
    Series *s = calloc(1, sizeof *s);
    if (s == NULL) {
        return NULL;
    }
    size_t name_len = strlen(name) + 1;
    s->name = malloc(name_len);
    s->values = calloc(len, sizeof *s->values);
    if (s->name == NULL || s->values == NULL) {
        series_free(s);              /* free(NULL) is a no-op, so partial init is safe */
        return NULL;
    }
    memcpy(s->name, name, name_len);
    s->len = len;
    return s;
}

void series_free(Series *s) {
    if (s == NULL) {
        return;
    }
    free(s->name);
    free(s->values);
    free(s);
}
```

- Check every allocation. `sizeof *p` instead of `sizeof(Type)` keeps the size right when the type changes. Never cast `malloc`'s result in C.
- Array sizes: `malloc(n * sizeof *p)` overflows `size_t` for a hostile `n`. Use `calloc(n, sizeof *p)` (it checks the multiplication) or check with `n > SIZE_MAX / sizeof *p`; C23 offers `ckd_mul`.
- `realloc` returns NULL on failure and leaves the old block allocated. Assigning it straight back leaks and loses the data:

```c
#include <stdint.h>
#include <stdlib.h>

int *grow(int *buf, size_t *cap) {
    if (*cap > SIZE_MAX / 2 || (*cap ? *cap * 2 : 16) > SIZE_MAX / sizeof *buf) {
        return NULL;
    }
    size_t new_cap = *cap ? *cap * 2 : 16; /* nonzero, including when cap is zero */
    int *tmp = realloc(buf, new_cap * sizeof *tmp);
    if (tmp == NULL) {
        return NULL;                 /* buf is still valid; the caller still owns it */
    }
    *cap = new_cap;
    return tmp;
}
```

- After `free`, the pointer is dangling. Set it to NULL when it lives on (`free(p); p = NULL;`), which turns a double free into a harmless no-op and a use after free into a crash at a known place.
- Cleanup with one exit path keeps early returns from leaking: acquire resources in order, and on failure `goto` a label chain that releases them in reverse (`out_buf: free(buf); out_file: fclose(f); return rc;`).
- A function that returns a pointer into a local array or a freed block returns a dangling pointer. Return heap memory the caller frees, or take a caller-supplied buffer and size.
- Stack frames are small: large arrays (`char buf[1 << 20]`) and unbounded recursion overflow it. Variable-length arrays (optional since C11) have no failure path; avoid them for sizes that come from input.

## Error Handling

C has no exceptions: a function reports failure through its return value, and the caller checks it.

| Convention | Use for |
| --- | --- |
| Return `0` on success, a negative value or `-errno` on failure | operations with an out-parameter result |
| Return a pointer, NULL on failure | constructors and lookups |
| Return a `bool` / `int`, result via `T *out` | parsing and conversion |
| An `enum` error code per module | a library with several failure kinds |

```c
#include <errno.h>
#include <limits.h>
#include <stdbool.h>
#include <stdlib.h>

/* Parses a decimal int. Returns false on empty input, trailing junk or overflow. */
bool parse_int(const char *s, int *out) {
    char *end = NULL;
    errno = 0;
    long v = strtol(s, &end, 10);
    if (end == s || *end != '\0' || errno == ERANGE || v < INT_MIN || v > INT_MAX) {
        return false;
    }
    *out = (int)v;
    return true;
}
```

- `errno` is meaningful only immediately after a call that reports failure through its return value. Set `errno = 0` before calls that signal errors through `errno` alone (`strtol`), copy it before calling anything else (`fprintf` can change it), and format with `strerror` (use `strerror_r` or a per-thread buffer in threaded code).
- Check what can fail: `fopen`, `fclose` (it flushes: its failure means lost writes), `fwrite`, `fread` (distinguish `feof` from `ferror`), `malloc`, `snprintf`, `close`, `write` (may write fewer bytes than asked; loop), `pthread_*`. Mark your own functions `[[nodiscard]]` (C23) or `__attribute__((warn_unused_result))`.
- POSIX calls interrupted by a signal return `-1` with `EINTR`; retry in a loop for `read`, `write`, `waitpid`.
- Do not use `atoi` or `atof` (no error reporting), `gets` (removed in C11), or `system` with a string built from input.
- Cleanup on error goes through the `goto` label chain described under Memory Ownership; `assert` is for programmer errors and vanishes under `NDEBUG`, so it never validates input.

## Undefined Behaviour

UB lets the compiler assume the case never happens, so the damage is not confined to the line that triggers it. Treat each entry as a bug even if the test passes.

| Operation | Why it is UB | Fix |
| --- | --- | --- |
| Signed integer overflow (`INT_MAX + 1`, `abs(INT_MIN)`) | the compiler may delete overflow checks written after the fact | check before: `if (a > INT_MAX - b)`; use unsigned for wraparound; C23 `ckd_add` |
| Out-of-bounds read or write | memory corruption or disclosure | track and check lengths; ASan in tests |
| Use after free, double free | allocator state is corrupt | one owner, null after free |
| NULL dereference | the compiler may delete later NULL checks on the same pointer | check at the boundary; `-fsanitize=null` |
| Uninitialised read | indeterminate value | initialise at declaration (`= {0}`) |
| Shift by negative or by `>= width`; left shift into the sign bit | result is undefined, and CPUs mask the count differently | mask the count; shift unsigned values |
| Division by zero; `INT_MIN / -1` | traps | check divisor |
| Pointer arithmetic outside an array (one past the end is allowed) | the pointer itself is invalid, even if never dereferenced | compare indices, not computed pointers |
| Strict aliasing: reading a `float` through `int *` | the optimiser reorders accesses | `memcpy`, or a `union` for type punning (defined in C) |
| Misaligned access through a cast pointer | traps on some CPUs, UB everywhere | `memcpy` |
| Writing to a string literal | literals are read-only | `char buf[] = "..."` for a writable copy |
| Two unsequenced modifications (`i = i++ + 1`, `f(i++, i++)`) | evaluation order is unspecified | one side effect per statement |
| `memcpy` with overlapping ranges | the copy direction is unspecified | `memmove` |
| `memcpy`, `memset` or `strlen` on a NULL pointer, even with length 0 | the compiler may assume the pointer is non-NULL afterwards | guard the NULL case |
| `char` value passed to `isalpha`, `toupper` (negative values) | argument must be `unsigned char` or `EOF` | `isalpha((unsigned char)c)` |
| `printf` with a wrong format specifier or argument count | reads arguments of the wrong size from the wrong place | `-Wformat=2`; `%zu` for `size_t`, `%td` for `ptrdiff_t`, `PRIu64` for `uint64_t` |
| Data race on a non-atomic object | torn reads and writes; the compiler may cache the value | `_Atomic`, mutex |
| Falling off the end of a non-`void` function whose result is used | the caller reads an indeterminate value | return on every path (`-Wreturn-type`) |

```c
/* Wrong: the optimiser may remove the check because signed overflow cannot happen */
bool add_overflows_bad(int a, int b) { return a + b < a; }

/* Right: test before adding */
bool add_overflows(int a, int b) {
    return (b > 0 && a > INT_MAX - b) || (b < 0 && a < INT_MIN - b);
}
```

- Mixed signed and unsigned comparison converts the signed side: `if (len < sizeof buf)` with a negative `int len` is false. Use `size_t` for sizes and indices and check signed values for negative first.
- `int` is at least 16 bits, `long` is 32 bits on Windows and 64 on Linux. Use `<stdint.h>` types (`int32_t`, `uint64_t`) for formats and `size_t` for sizes.

## Strings and Buffers

A C string is a `char` array ending in `'\0'`; every function that reads one needs the terminator, and every function that writes one needs a size.

| Do not use | Use |
| --- | --- |
| `gets`, `scanf("%s")` | `fgets(buf, sizeof buf, stdin)`, or `getline` (POSIX) |
| `strcpy`, `strcat`, `sprintf` | `snprintf` with the buffer size, or a counted copy that terminates |
| `strncpy` (does not terminate when the source fills the buffer, and zero-fills the rest) | `snprintf(dst, sizeof dst, "%s", src)`, or `memcpy` then terminate |
| `strtok` (hidden static state) | `strtok_r`, or `strchr` and `memcpy` |
| `atoi` | `strtol` (see Error Handling) |

```c
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

/* Copies src into dst (capacity cap). Returns false if truncation would occur. */
bool copy_str(char *dst, size_t cap, const char *src) {
    size_t n = strlen(src);
    if (cap == 0 || n >= cap) {
        return false;
    }
    memcpy(dst, src, n + 1);
    return true;
}

int format_id(char *dst, size_t cap, const char *prefix, unsigned id) {
    int n = snprintf(dst, cap, "%s-%u", prefix, id);
    if (n < 0 || (size_t)n >= cap) {
        return -1;                   /* encoding error or truncation */
    }
    return n;
}
```

- `snprintf` returns the length it would have written, so `n >= cap` detects truncation. `sizeof buf` gives the capacity only for an array in the same scope, not for a parameter (`void f(char buf[256])` is a pointer): pass the size explicitly.
- Pass `(pointer, length)` pairs, or a `struct { const char *ptr; size_t len; }` view, so a function never calls `strlen` on non-terminated data. Embedded NUL bytes end a C string early.
- Never pass user input as the format: `printf(user)` is a format-string vulnerability; write `printf("%s", user)`.
- Read whole files with `fread` into a buffer sized from `fstat`, or grow with the `realloc` pattern; `ftell` on a text stream and `gets`-style fixed buffers are traps.
: `char` is bytes, and UTF-8 is a byte encoding. `strlen` counts bytes, and `toupper` does not handle non-ASCII. Use a library (ICU, utf8proc) for text processing.

## Headers and Modules

```c
/* include/spool.h */
#ifndef SPOOL_H
#define SPOOL_H

#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Spool Spool;          /* opaque: the layout lives in spool.c */

Spool *spool_open(const char *dir);
bool spool_append(Spool *s, const void *rec, size_t len);
void spool_close(Spool *s);

#ifdef __cplusplus
}
#endif

#endif /* SPOOL_H */
```

- Include guards (`#ifndef NAME_H`) or `#pragma once` (supported by every mainstream compiler, not in the standard). Guard names are unique per project: `SPOOL_H`, not `UTIL_H`.
- A header declares; it does not define. A non-`static` function or variable defined in a header causes duplicate symbols at link time. Small `static inline` functions are fine in headers.
- Each header includes what it needs and compiles alone. Put a `#include "spool.h"` first in `spool.c` so a missing include in the header shows up.
- Opaque types (`typedef struct Spool Spool;`) hide the layout and let it change without recompiling callers; expose `*_open`/`*_close` functions.
- `static` on file-scope functions and variables gives internal linkage. Share a global through one `extern` declaration in a header and one definition in one `.c` file, and prefer passing a context struct.
- Prefix exported names with the module (`spool_open`), since C has no namespaces. Macros are all-caps and wrap arguments in parentheses; a macro that evaluates its argument twice (`#define MAX(a, b) ((a) > (b) ? (a) : (b))`) misbehaves with `MAX(i++, j)`. Prefer `static inline` functions and `enum` constants.
 with a `void *ctx` argument so callers carry state without globals.

## Sanitizers and Analysis

| Tool | How | Finds |
| --- | --- | --- |
| AddressSanitizer | `-fsanitize=address -fno-omit-frame-pointer` | heap/stack/global overflow, use after free, double free, leaks (Linux) |
| UndefinedBehaviorSanitizer | `-fsanitize=undefined` (add `-fno-sanitize-recover=all` so the first report fails the run) | signed overflow, shifts, misaligned and null access, bad enum values |
| ThreadSanitizer | `-fsanitize=thread` | data races |
| Valgrind | `valgrind --leak-check=full --track-origins=yes ./test` | leaks and invalid access on an unsanitised binary; 10-50x slower |
| clang-tidy | `.clang-tidy` with `clang-analyzer-*`, `bugprone-*`, `cert-*` | unsafe calls, suspicious expressions |
| cppcheck | `--enable=warning,style,performance --error-exitcode=1` | leaks, null derefs, buffer indices |

- Run the test suite under ASan and UBSan in CI on every change (`cmake -S . -B build-asan -DCMAKE_C_FLAGS='-fsanitize=address,undefined -fno-omit-frame-pointer'`), with `ASAN_OPTIONS=detect_leaks=1:abort_on_error=1`.
: `-D_FORTIFY_SOURCE=3 -fstack-protector-strong -fPIE -pie -Wl,-z,relro,-z,now`.

## Testing

| Framework | Fits |
| --- | --- |
| CTest alone | small suites: one executable per test, `main` returns non-zero on failure |
| Unity | embedded and minimal-dependency projects; pairs with CMock |
| cmocka | mocks and fixtures in plain C, `cmocka_run_group_tests` |

```c
/* tests/parse_test.c: cmocka */
#include <stdarg.h>
#include <stddef.h>
#include <setjmp.h>
#include <cmocka.h>
#include "parse.h"

static void parse_int_accepts_decimal(void **state) {
    (void)state;
    int v = 0;
    assert_true(parse_int("42", &v));
    assert_int_equal(v, 42);
}

static void parse_int_rejects_trailing_junk(void **state) {
    (void)state;
    int v = 0;
    assert_false(parse_int("42px", &v));
}

int main(void) {
    const struct CMUnitTest tests[] = {
        cmocka_unit_test(parse_int_accepts_decimal),
        cmocka_unit_test(parse_int_rejects_trailing_junk),
    };
    return cmocka_run_group_tests(tests, NULL, NULL);
}
```

- A CTest name selects a whole executable. cmocka runs a group of cases per executable, so `ctest -R` cannot pick one case inside it; when a loom contract targets a single case, give that case its own executable and `add_test` entry.
 by wrapping `malloc` (cmocka `--wrap=malloc` linker flag, or a test allocator that fails the Nth call).
- Tests cover boundaries: empty input, a buffer of exactly `cap - 1`, `cap` and `cap + 1` bytes, `INT_MAX`, `SIZE_MAX`, NULL.

## Verification Checklists

**Before marking C work done:**

- [ ] Builds with `-Wall -Wextra -Wpedantic -Wshadow -Wconversion` and no warnings; `-Werror` clean in CI
- [ ] The tests for the changed code pass under ASan and UBSan (`-fno-sanitize-recover=all`), and under `-O2`
- [ ] Every allocation, `fopen`, `fclose`, `fwrite` and `snprintf` result is checked; no ignored return values
- [ ] Each heap block has one named owner and one free; `realloc` never assigned straight back
- [ ] No `strcpy`, `strcat`, `sprintf`, `gets`, `atoi`; every buffer write carries a size
- [ ] Size arithmetic checked for overflow before `malloc`; indices are `size_t` and compared as unsigned
- [ ] No signed overflow, shifts out of range, `ctype` calls with a plain `char`, or format-string use of input
- [ ] Headers have include guards, compile alone and define nothing non-`static`
- [ ] `clang-tidy` or `cppcheck` run with no new findings; `valgrind` clean when sanitizers are not available

## Loom Test Runner Adapter

**Adapter.** `ctest`, and only for a package with a `CMakeLists.txt`. Loom has no `c` language profile and no C-specific adapter. `loom project detect` marks a directory `cpp` when it holds a `CMakeLists.txt` (`loom/src/skills/project/markers.rs:27`) and every `cpp` package gets `ctest` (`loom/src/skills/project/runners.rs:47`), so a C project that builds with CMake is run by the same adapter as a C++ one, whatever `project(... LANGUAGES C)` says.

```text
spool  kinds=cpp  runner=ctest
```

**Without a `CMakeLists.txt`** (a plain `Makefile`, `meson.build`, `configure` or Bazel build) the directory gets no kind from those files, no runner, and so no single-test command. A contract in such a package cannot be frozen through loom. The only route to a loom-run test is a `CMakeLists.txt` that registers the tests with `add_test`.

**Single-test command**, run with the package directory as cwd (`loom/src/testrun/adapters/ctest.rs:79`):

```bash
cmake --build build && ctest --test-dir build -R '^{test}$' --output-on-failure
```

CTest never compiles; the `cmake --build build &&` prefix makes a stale binary impossible and lets a compile error stop the pipeline before `ctest`. `cmake --build` does not configure, so the package directory needs a binary directory named `build` that is already configured (`cmake -S . -B build`, or a preset whose `binaryDir` is `${sourceDir}/build`).

**The `test` field** is the `add_test` name exactly as `ctest --test-dir build -N` prints it. Loom anchors it as a regular expression, so keep it to letters, digits, `_`, `.`, `/` and spaces; regex metacharacters change what matches and a `'` breaks the shell quoting (the adapter escapes and quotes, but a metacharacter-free name is verifiable by hand). A name matching nothing makes `ctest` exit 0 with `No tests were found!!!`; loom reads that as "the runner did not select the test" (`ctest.rs:94`) and fails the freeze.

**Test-file recognition.** The nearest profile, `cpp` (`loom/src/testrun/languages.rs:113-123`), matches `*_test.cpp`, `*_test.cc`, `test_*.cpp` and `tests/**/*.cpp`, with `TEST(`, `TEST_F(`, `TEST_P(`, `TEST_CASE(` as declarations. A C test file (`tests/spool_test.c`) matches no profile, so loom neither counts its tests nor treats it as a test file when it selects impacted tests (`verify/integrity/count.rs:63`, `verify/impact_tests.rs:159`). The contract still runs through `ctest`, and the `file` field still identifies the owning package and the frozen path; do not rely on the declaration and assertion checks to cover a `.c` file.

**Writing contract tests:**

- Register tests from the top-level `CMakeLists.txt`, or from a file it `include()`s, so the package that owns the contract file is the configured project root. A `tests/CMakeLists.txt` pulled in with `add_subdirectory(tests)` makes `tests/` a package of its own with no `build/` inside it.
- One CTest name per case; with cmocka or Unity that means one executable per case (see Testing).
- A new test source joins an `add_executable` list. That CMake edit is harness: list it in the stage's `harness` globs. Keep `build/` in `.gitignore`.

```yaml
harness:
  - tests/tests.cmake
contracts:
  - id: parse-int-rejects-trailing-junk
    file: tests/parse_int_junk_test.c
    test: parse_int_rejects_trailing_junk
    scenario: calls parse_int on the string "42px" and checks the return value and that the output int is untouched
    rejects: a parse_int built on atoi or on strtol without the end-pointer check, which accepts the trailing junk
```

**Build failures:** a contract test calling a function the stage has not written yet fails `cmake --build` (compiler diagnostics, exit 2); `&&` skips `ctest`, loom classifies the run as a build failure, and the freeze accepts that as red. A function that is declared but not defined fails at link time and counts the same. Because `cmake --build build` builds every target, an unrelated compile error elsewhere also reads as red: check that the diagnostics name the API the contract is waiting for.
