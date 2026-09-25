---
name: loom-php
description: PHP language expertise for idiomatic, production-quality code.
triggers:
  - php
  - composer
  - packagist
  - psr-4
  - phpunit
  - pest
  - laravel
  - eloquent
  - artisan
  - symfony
  - doctrine
  - phpstan
  - psalm
  - php-cs-fixer
  - pint
  - rector
  - octane
  - php-fpm
---

# PHP Language Expertise

## Overview

Idiomatic, production-grade PHP 8.x: Composer, strict types, enums and readonly, the Laravel and Symfony essentials, and PHPUnit/Pest. Assumes competence; the value is in what is easy to get wrong (coercive typing, loose comparison, the by-reference `foreach` trap, state leaking in long-running workers, N+1 queries, unsafe `unserialize`) and in writing tests that loom's `phpunit` and `pest` adapters can select one at a time.

## Tooling

**Composer** manages dependencies and generates the autoloader.

| Task | Command |
| --- | --- |
| Add a dependency | `composer require guzzlehttp/guzzle` (updates `composer.json` and `composer.lock`) |
| Add a dev dependency | `composer require --dev phpunit/phpunit` |
| Install from the lock | `composer install` (CI: `--no-interaction --prefer-dist`) |
| Production install | `composer install --no-dev --classmap-authoritative` |
| Update one package | `composer update vendor/package --with-dependencies` |
| Regenerate the autoloader | `composer dump-autoload` (after editing `autoload`) |
| Check the manifest | `composer validate --strict` |
| Known vulnerabilities | `composer audit` |

- Commit `composer.lock` for applications. Add and remove packages with `composer require`/`composer remove`; the `autoload`, `config` and `scripts` sections are edited by hand.
- `config.platform.php` pins the PHP version used for resolution, so a newer local PHP cannot lock packages the server cannot run.
- PSR-4: one class per file, namespace prefix mapped to a directory; a class whose path does not match its namespace fails to autoload only when first used.

```json
{
    "require": { "php": "^8.3" },
    "autoload": { "psr-4": { "App\\": "src/" } },
    "autoload-dev": { "psr-4": { "Tests\\": "tests/" } },
    "config": { "platform": { "php": "8.3.0" }, "sort-packages": true },
    "scripts": { "test": "phpunit", "analyse": "phpstan analyse" }
}
```

**Format:** PHP-CS-Fixer (`vendor/bin/php-cs-fixer fix`), Laravel Pint (`vendor/bin/pint`, PHP-CS-Fixer with Laravel presets), or PHP_CodeSniffer (`phpcs`/`phpcbf`) with PSR-12 / PER Coding Style.

**Static analysis:** PHPStan (`vendor/bin/phpstan analyse`; levels 0 to 10 in PHPStan 2, `max` is the highest) or Psalm; Larastan adds Laravel's magic. Adopting on legacy code, generate a baseline (`--generate-baseline`) and shrink it; never add new errors to it. Generics exist only in docblocks for these tools: `@param list<User>`, `@return array<string, int>`, `@template T`.

**Upgrades:** Rector (`vendor/bin/rector process --dry-run`) automates version migrations and PHPUnit attribute conversion.

**The gate:**

```bash
composer validate --strict
vendor/bin/php-cs-fixer fix --dry-run --diff     # or: vendor/bin/pint --test
vendor/bin/phpstan analyse
vendor/bin/phpunit                               # or: vendor/bin/pest
composer audit
```

## Types and Strictness

Start every file with `declare(strict_types=1);`. Without it scalar parameters coerce (`"42"` becomes `42`, `true` becomes `1`). The mode applies to calls made *from* the declaring file, so a strict library called from a non-strict file still receives coerced values.

| Version | Type features |
| --- | --- |
| 8.0 | Union types `int\|string`, `mixed`, `static` return, constructor promotion, named arguments, `match`, nullsafe `?->`, `throw` as an expression, attributes |
| 8.1 | Enums, readonly properties, `never`, intersection types `A&B`, `new` in initializers, callable syntax `strlen(...)`, fibers |
| 8.2 | Readonly classes, DNF types `(A&B)\|null`, standalone `true`/`false`/`null`, dynamic properties deprecated |
| 8.3 | Typed class constants, `#[\Override]`, `json_validate()` |
| 8.4 | Property hooks, asymmetric visibility `public private(set)`, `new Foo()->method()` without wrapping parentheses; implicit nullable parameters (`Foo $x = null`) deprecated, write `?Foo $x = null` |

```php
<?php

declare(strict_types=1);

namespace App\Spool;

final class Writer
{
    public function __construct(
        private readonly string $root,
        private readonly Clock $clock = new SystemClock(),   // new in initializer (8.1)
    ) {}

    /** @param list<string> $lines */
    public function write(string $jobId, array $lines): int
    {
        if (is_link($this->root)) {
            throw new SymlinkException("spool root is a symlink: {$this->root}");
        }

        return $this->append($this->root . '/' . $jobId, $lines, $this->clock->now());
    }
}
```

## Enums and Readonly

- Pure enums (`enum Suit { case Hearts; ... }`) and backed enums (`enum Status: string`). `Status::from($v)` throws `ValueError` on an unknown value; `Status::tryFrom($v)` returns `null`. `Status::cases()` lists them. Enums take methods, constants and interfaces; they hold no state.
- `match` compares with `===` and throws `UnhandledMatchError` when no arm matches. A `match` over an enum without `default` fails loudly when a case is added; keep it that way.
- Readonly properties are written once, from inside the class, and must be typed. They are shallow: an object held in a readonly property stays mutable. Readonly classes (8.2) make every promoted and declared property readonly and forbid dynamic properties.
- Modify a readonly value object by constructing a new one (`withCurrency()` returning `new self(...)`); 8.3 also allows reinitializing readonly properties inside `__clone`.
- Property hooks (8.4) replace most getter/setter pairs; asymmetric visibility (`public private(set)`) gives public reads with private writes.

```php
enum Status: string
{
    case Pending = 'pending';
    case Active = 'active';
    case Suspended = 'suspended';

    public function canLogIn(): bool
    {
        return match ($this) {
            self::Active => true,
            self::Pending, self::Suspended => false,
        };
    }
}

$status = Status::tryFrom($input) ?? throw new \InvalidArgumentException("unknown status: {$input}");

final readonly class Money
{
    public function __construct(public int $cents, public string $currency) {}

    public function add(self $other): self
    {
        if ($other->currency !== $this->currency) {
            throw new \LogicException('currency mismatch');
        }
        return new self($this->cents + $other->cents, $this->currency);
    }
}

final class Account
{
    public string $email {
        set => strtolower(trim($value));        // 8.4 set hook
    }

    public function __construct(public private(set) string $id, string $email)
    {
        $this->email = $email;
    }
}
```

## Errors and Exceptions

- `Throwable` splits into `Error` (engine: `TypeError`, `ValueError`, `ArgumentCountError`, `UnhandledMatchError`, `DivisionByZeroError`) and `Exception`. Catch `Exception` subclasses for recoverable conditions; catch `Throwable` only in the top-level handler.
- Give a library a marker interface (`interface SpoolException extends \Throwable {}`) and concrete classes extending SPL exceptions (`final class SymlinkException extends \RuntimeException implements SpoolException`). Callers catch the interface for everything or a class for one case.
- Chain the cause: `throw new SpoolWriteException('write failed', previous: $e);`.
- `json_decode($s, true, 512, JSON_THROW_ON_ERROR)` and `json_encode($v, JSON_THROW_ON_ERROR)`: without the flag, bad input returns `null`/`false` and the error hides in `json_last_error()`.
- `preg_*` functions return `false` (or `null`) on a regex error such as backtrack-limit exhaustion; check the return and `preg_last_error_msg()`.
- A `return` inside `finally` replaces both the `try` block's return value and any in-flight exception.
- Never silence errors with `@`; the failure disappears from logs and the code carries on with a bad value.

## Concurrency and the Request Model

- PHP-FPM is share-nothing: every request starts with fresh statics, singletons and globals, so per-request leaks vanish at the end of the request.
- Long-running processes (Laravel Octane, RoadRunner, FrankenPHP worker mode, Swoole, queue workers, Messenger consumers) keep the process across requests or jobs. Static properties, container singletons holding request data, and in-memory caches leak between requests, and memory grows. Reset request state explicitly, and restart workers periodically (`php artisan queue:work --max-jobs=1000 --max-time=3600`, `messenger:consume --limit=1000 --memory-limit=256M`).
- Fibers (8.1) are the primitive under AMPHP v3, ReactPHP and Revolt; application code rarely touches them directly.
- Parallel or deferred work goes through a queue (Laravel queues, Symfony Messenger) or separate processes (`symfony/process`).

## Laravel Essentials

- **N+1:** eager load with `with('customer')`; call `Model::preventLazyLoading(! app()->isProduction())` (or `Model::shouldBeStrict()`, which also rejects silently discarded and missing attributes) in `AppServiceProvider::boot`.
- **Mass assignment:** `$fillable` is the allowlist. Pass `$request->validated()` from a FormRequest; never `$guarded = []` together with `$request->all()`.
- **Configuration:** call `env()` only inside `config/*.php`. After `php artisan config:cache`, `env()` elsewhere returns `null`; read `config('services.stripe.key')`.
- **Transactions and queues:** `DB::transaction(fn () => ...)`; dispatch jobs from inside it with `->afterCommit()` so a worker never sees uncommitted rows. `SerializesModels` re-fetches models by key when the job runs; handlers must be idempotent (`$tries`, `backoff()`, `ShouldBeUnique`).
- **Large tables:** `chunkById` / `lazyById`; `chunk` while updating the filtered column skips rows.
- **Laravel 11+** configures middleware, exceptions and routing in `bootstrap/app.php`.
- **Tests:** `RefreshDatabase`, `Http::fake()`, `Queue::fake()`, `Mail::fake()`, `$this->actingAs($user)`.

```php
public function store(StoreOrderRequest $request): JsonResponse
{
    $order = DB::transaction(function () use ($request): Order {
        $order = Order::create($request->safe()->except('lines'));
        $order->lines()->createMany($request->validated('lines'));
        SendOrderConfirmation::dispatch($order)->afterCommit();

        return $order;
    });

    return OrderResource::make($order->load('lines'))->response()->setStatusCode(201);
}
```

## Symfony Essentials

- Services are autowired and autoconfigured by default (`config/services.yaml`); inject through constructors. Tag with attributes: `#[AsEventListener]`, `#[AsMessageHandler]`, `#[AsCommand]`.
- Routes via `#[Route]`; `#[MapRequestPayload]` (6.3+) deserializes and validates a request DTO.
- Doctrine: `persist()` schedules, `flush()` writes every pending change in one transaction. Flush once per unit of work; for imports, flush and `clear()` in batches or memory grows with the identity map. Lazy associations cause N+1; fetch-join in the query (`->addSelect('l')`).
- Authorization through voters (`#[IsGranted('EDIT', subject: 'order')]`); forms carry CSRF tokens by default.
- Diagnose with `bin/console debug:container`, `debug:router`, `lint:container`.

```php
#[Route('/api/orders', methods: ['POST'])]
public function create(#[MapRequestPayload] CreateOrder $command, MessageBusInterface $bus): JsonResponse
{
    $bus->dispatch($command);

    return new JsonResponse(null, Response::HTTP_ACCEPTED);
}
```

## Testing

**PHPUnit (10+).**

```php
<?php

declare(strict_types=1);

namespace Tests\Unit\Spool;

use App\Spool\SymlinkException;
use App\Spool\Writer;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class WriterTest extends TestCase
{
    private string $root;

    protected function setUp(): void
    {
        $this->root = sys_get_temp_dir() . '/spool-' . bin2hex(random_bytes(4));
        mkdir($this->root);
    }

    public function testRejectsSymlinkedSpoolDirectory(): void
    {
        mkdir($this->root . '/target');
        symlink($this->root . '/target', $this->root . '/spool');

        $this->expectException(SymlinkException::class);
        (new Writer($this->root . '/spool'))->write('job-1', ['payload']);
    }

    #[DataProvider('invalidJobIds')]
    public function testRejectsInvalidJobId(string $jobId): void
    {
        $this->expectException(\InvalidArgumentException::class);
        (new Writer($this->root))->write($jobId, ['payload']);
    }

    public static function invalidJobIds(): iterable
    {
        yield 'empty' => [''];
        yield 'traversal' => ['../etc/passwd'];
    }
}
```

- Metadata is attributes: `#[Test]`, `#[DataProvider]`, `#[CoversClass]`, `#[Group]`. Doc-comment annotations are deprecated in PHPUnit 11 and removed in 12.
- Data providers are `public static` and may `yield` named cases.
- `assertSame` compares with `===`; `assertEquals` is loose (`'1'` equals `1`). Default to `assertSame`.
- Call `expectException` before the code that throws; the rest of the test after the throw never runs.
- `createStub` when the test needs return values; `createMock` with `expects()` when it asserts calls. Recent PHPUnit versions flag a mock with no expectations.

**Pest.**

```php
<?php

use App\Spool\SymlinkException;
use App\Spool\Writer;

beforeEach(function () {
    $this->root = sys_get_temp_dir() . '/spool-' . bin2hex(random_bytes(4));
    mkdir($this->root);
});

test('rejects a symlinked spool directory', function () {
    mkdir($this->root . '/target');
    symlink($this->root . '/target', $this->root . '/spool');

    expect(fn () => (new Writer($this->root . '/spool'))->write('job-1', ['payload']))
        ->toThrow(SymlinkException::class);
    expect(file_exists($this->root . '/target/job-1'))->toBeFalse();
});

it('accepts a plain directory', function (string $jobId) {
    expect((new Writer($this->root))->write($jobId, ['payload']))->toBeGreaterThan(0);
})->with(['job-1', 'job-2']);
```

- `tests/Pest.php` binds base test cases per directory: `pest()->extend(Tests\TestCase::class)->in('Feature');` (Pest 3; Pest 2 uses `uses(...)->in(...)`).
- `it('does x')` names the test `it does x`; `test('does x')` names it `does x`.
- `->with([...])` datasets, `beforeEach`/`afterEach`, `arch()->expect('App')->not->toUse(['dd', 'dump'])` architecture rules.
- A stray `->only()` makes the whole run execute that test alone; CI should reject it.
- Pest runs on PHPUnit: existing PHPUnit classes run unchanged under `vendor/bin/pest`.

## Loom Test Runner Adapter

**Adapter.** Two adapters cover PHP; `loom project detect` prints the one chosen for each package (kind `php`, marker `composer.json`):

- `pest` when `composer.json` requires `pestphp/pest` (normally in `require-dev`);
- `phpunit` otherwise.

**Single-test command.** Loom runs it with the package directory as cwd:

```bash
vendor/bin/phpunit --filter '{test}' {file}
vendor/bin/pest --filter '{test}' {file}
```

**The `test` field.** The test method name for `phpunit` (`testRejectsSymlinkedSpoolDirectory`), or the Pest description (`rejects a symlinked spool directory`; an `it()` test's description starts with `it`).

**No-match behaviour.** Both runners are documented: no fixture was captured, and loom's parsers were written from each runner's documented output (PHPUnit: `OK (N tests, A assertions)`, `Tests: N, Assertions: A, Failures: F`, `No tests executed!`; Pest: `Tests:  F failed, P passed (A assertions)`, `No tests found`). PHPUnit's exit code for an empty run depends on the version and on `--fail-on-empty-test-suite`. Loom reads the runner's summary, so zero executed tests is `NotSelected` whatever the exit code: a contract whose `test` value does not match fails the freeze ("the runner did not select the test").

**Writing contract tests.**

- Keep contracts under `tests/` in files ending `Test.php` (`tests/Unit/Spool/WriterTest.php`), PHPUnit's default suffix, which Pest projects keep. Loom maps a file to its language profile by extension and test-file glob.
- ⚠ `--filter` is a pattern. PHPUnit wraps a plain value into a case-insensitive regular expression and matches it anywhere in `Class::method` (plus the data-set suffix); Pest hands the filter to PHPUnit. Consequences:
  - make the name unique in its file and not a prefix of another test (`testRejectsSymlink` also selects `testRejectsSymlinkLoop`);
  - keep Pest descriptions to letters, digits, spaces, `-` and `_`: `(`, `)`, `.`, `?`, `+`, `*` and `[` are regex syntax, and a `#` or `@` in the filter selects a data set;
  - loom single-quotes the value, so no single quotes either.
- Give a contract no data provider or dataset: the filter runs every data set, and one contract should pin one scenario.
- Put a Pest contract in the directory whose `tests/Pest.php` binding supplies the base test case it needs. A Feature test outside `Feature/` loses Laravel's `TestCase`, fails for the wrong reason at freeze, and keeps failing after the implementation.
- Reference new classes only inside test bodies. A `use` import of a class that does not exist yet is harmless (PHP resolves it on first use), and instantiating it inside the test raises an `Error` that PHPUnit reports as a test error with a non-zero exit: red.

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: tests/Unit/Spool/WriterTest.php
    test: testRejectsSymlinkedSpoolDirectory
    scenario: creates a temp directory, symlinks the spool root to it, calls Writer::write()
    rejects: a writer that follows the symlink and writes job-1 into the link target
```

The Pest form of the same contract uses `test: "rejects a symlinked spool directory"`. Quote YAML values that contain `:` or `#`.

**Build failures.** PHP has no separate build step, so a contract never reports `BuildFailed`. A parse error in the test file stops the run before any summary; loom then treats the run as unparsed and requires a non-zero exit. Keep contract files syntactically valid and let them fail inside the test.

Loom's test-impact runs cannot select PHPUnit or Pest tests by file (`select_command` returns none for both); the full suite runs in integration-verify.

## Anti-Patterns

```php
// A by-reference foreach leaves $row bound to the last element
foreach ($rows as &$row) {
    $row['total'] = $row['qty'] * $row['price'];
}
unset($row);                              // GOOD: break the reference right after the loop
// Without that unset, a later foreach ($rows as $row) writes each value into the last element
$rows = array_map(fn (array $r) => [...$r, 'total' => $r['qty'] * $r['price']], $rows);  // better

// Loose comparison
in_array('1e1', ['10']);                  // true: numeric strings compare as numbers
in_array($needle, $haystack, true);       // GOOD: strict
if (strpos($haystack, 'id') == false) {}  // BAD: position 0 is falsy
if (!str_contains($haystack, 'id')) {}    // GOOD (8.0)
```

Quick swaps:

- `==` / `switch` ⇒ `===` / `match`.
- `array_search(...) == false`, `empty('0')` (true) ⇒ explicit `=== false`, `=== ''` checks.
- `DateTime` ⇒ `DateTimeImmutable` (the mutable one changes under every caller that holds it).
- `array_merge` inside a loop (quadratic) ⇒ collect arrays, then `array_merge(...$chunks)` once.
- `rand()`/`mt_rand()`/`uniqid()` for tokens ⇒ `random_bytes()` / `random_int()`.
- `extract()`, variable variables, `$GLOBALS` ⇒ explicit variables and parameters.
- `catch (\Exception $e) { return null; }` ⇒ catch the specific class, or let it propagate.
- Facades and `app()` deep inside domain classes ⇒ constructor injection.
- String-built SQL ⇒ bound parameters.

## Expert Practices

### Idioms

- Declare classes `final` unless they are designed for extension; prefer small interfaces and composition.
- Named constructors (`Money::fromString()`) for alternate construction; `private function __construct` when every construction must go through them.
- `static fn () => ...` for closures that never use `$this` (no implicit binding); arrow functions capture outer variables by value automatically.
- Generators (`yield`) stream large result sets and files in constant memory.
- `WeakMap` (8.0) for per-object caches that must not keep objects alive; `array_is_list()` (8.1) to validate list input.

### Gotchas

- Arrays are values (copy-on-write); objects are handles. Passing an array and modifying it changes a copy; passing an object shares it.
- `array_filter` preserves keys, so the result is no longer a list; wrap it in `array_values()` before `json_encode` or `list<T>` code.
- `json_encode([])` is `[]` while an empty object is `{}`; use `new \stdClass()` or `JSON_FORCE_OBJECT` where the consumer expects an object.
- `array_merge` renumbers integer keys; `$a + $b` keeps the left side's keys and drops duplicates from the right.
- `isset($a['k'])` is false for a `null` value; `array_key_exists('k', $a)` checks presence.
- `self::` binds to the defining class, `static::` to the called class (late static binding).
- String functions count bytes; use the `mb_*` family for user text.
- Integers overflow to float silently past `PHP_INT_MAX`.

### Performance

- OPcache on in production, with `opcache.validate_timestamps=0` and a reload on deploy. The JIT (`opcache.jit`) helps CPU-bound code only; typical web requests are IO-bound.
- `composer install --classmap-authoritative` skips filesystem lookups for classes.
- Most PHP web latency is queries: eager loading, selecting only needed columns, and indexes matter more than micro-optimizing PHP.
- Profile with Xdebug's profiler, Blackfire, or SPX before tuning.

### Security

- SQL: PDO prepared statements with `PDO::ATTR_EMULATE_PREPARES => false` and `PDO::ERRMODE_EXCEPTION`; column and sort names from input go through an allowlist.
- Passwords: `password_hash($p, PASSWORD_DEFAULT)`, `password_verify()`, `password_needs_rehash()`. Tokens: `bin2hex(random_bytes(32))`. Compare secrets with `hash_equals()`.
- Output: `htmlspecialchars($s, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8')`; Blade `{{ }}` and Twig escape, `{!! !!}` and `|raw` do not.
- `unserialize()` on untrusted data is object injection (gadget chains run on `__wakeup`/`__destruct`); use JSON, or `unserialize($s, ['allowed_classes' => false])`.
- Stream wrappers: `file_get_contents($userUrl)` also accepts `php://`, `phar://` and `file://`; validate the scheme and host before fetching (SSRF).
- Shell: Symfony `Process` with an argument array, or `escapeshellarg()` for each argument.
- Paths: `include`/`require` never take user input; resolve user paths with `realpath()` and check the prefix.
- Sessions: `session_regenerate_id(true)` on login; cookies `Secure`, `HttpOnly`, `SameSite`.

## Verification Checklists

**Before marking PHP work done:**

- [ ] `declare(strict_types=1);` in every new file
- [ ] Formatter clean (`php-cs-fixer fix --dry-run --diff` or `pint --test`)
- [ ] `phpstan analyse` (or Psalm) passes at the project's level with no new baseline entries
- [ ] `vendor/bin/phpunit` / `vendor/bin/pest` green; new behaviour has tests using `assertSame`/`toBe`
- [ ] No loose comparisons on user data; `in_array`/`array_search` strict
- [ ] `JSON_THROW_ON_ERROR` on every `json_decode`/`json_encode` of external data
- [ ] Dependencies added with `composer require`; `composer.lock` committed; `composer audit` clean
- [ ] No `unserialize`, `extract` or string-built SQL on untrusted input

**Framework review:**

- [ ] No lazy loading on new list endpoints (`with()`, `preventLazyLoading`, Doctrine fetch joins)
- [ ] Mass assignment goes through `validated()` data or an explicit field list
- [ ] Jobs dispatched after commit and idempotent; long-running workers reset request state
- [ ] `env()` only in config files

**Contract tests for loom:**

- [ ] `test` value is the method name or Pest description, unique in its file, not a prefix of another test, free of regex syntax, `#`, `@` and single quotes
- [ ] No data provider or dataset on the contract test; Pest contract sits under the directory whose binding it needs
- [ ] The contract fails before the implementation for the reason its `rejects` field names
