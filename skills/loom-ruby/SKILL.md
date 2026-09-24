---
name: loom-ruby
description: Ruby language expertise for idiomatic, production-quality code.
triggers:
  - ruby
  - rb
  - bundler
  - gem
  - gemfile
  - gemspec
  - rake
  - rails
  - activerecord
  - activejob
  - rspec
  - minitest
  - rubocop
  - standardrb
  - sorbet
  - rbs
  - sidekiq
  - puma
  - sinatra
  - yjit
---

# Ruby Language Expertise

## Overview

Idiomatic, production-grade Ruby 3.x: Bundler, the Ruby 3 language additions, blocks and closures, Rails, and RSpec/Minitest. Assumes competence; the value is in what is easy to get wrong (keyword-argument separation, `proc` return semantics, `||=` on falsy values, N+1 queries, unsafe deserialization, the GVL) and in writing tests that loom's `rspec` and `minitest` adapters can select one at a time.

## Tooling

Preserve the repository's version manager and lockfile. The Ruby version lives in `.ruby-version` (read by rbenv, chruby, asdf, mise) and in the `Gemfile`'s `ruby` directive.

**Bundler** resolves dependencies and isolates the load path.

| Task | Command |
| --- | --- |
| Add a dependency | `bundle add faraday` (edits `Gemfile`, resolves, updates `Gemfile.lock`) |
| Add a dev/test dependency | `bundle add rspec --group "development, test"` |
| Install from the lock | `bundle install`; set `BUNDLE_FROZEN=true` in CI so drift fails the build |
| Update one gem | `bundle update --conservative faraday` (shared dependencies stay pinned) |
| Run inside the bundle | `bundle exec rspec`, or the binstubs `bin/rails`, `bin/rspec` |
| Known CVEs in the lock | `bundle exec bundler-audit check --update` |
| Deploy platforms | `bundle lock --add-platform x86_64-linux` |

- `bundle exec` activates exactly the locked versions; a bare `rspec` or `rake` may load a different installed version. `require "bundler/setup"` does the same from inside a script.
- A gem declares runtime dependencies in its `.gemspec`; its `Gemfile` holds `gemspec` plus development tools.

**Format and lint:** RuboCop (`rubocop -a` applies safe corrections; `-A` adds unsafe ones, so review that diff) or Standard (`standardrb --fix`, RuboCop with a fixed config). Rails 7.2+ generates `bin/rubocop` (rubocop-rails-omakase) and `bin/brakeman`. Plugins: `rubocop-rspec`, `rubocop-performance`, `rubocop-rails`. Disable a cop for a range with `# rubocop:disable Style/Foo` ... `# rubocop:enable Style/Foo`; never disable all cops for a file.

**Types (gradual, optional):** RBS signatures in `sig/*.rbs` checked by Steep, or Sorbet (`# typed: strict`, `sig { params(id: Integer).returns(String) }`, `srb tc`). Follow whichever the project uses; never add a second one.

**The gate:**

```bash
bundle exec rubocop                 # or: bundle exec standardrb
bundle exec srb tc                  # Sorbet projects only (Steep: bundle exec steep check)
bundle exec rspec                   # or: bin/rails test, bundle exec rake test
bundle exec brakeman --no-pager     # Rails apps: static security scan
```

## Ruby 3 Language Features

`.ruby-version` bounds what you may use.

| Version | Feature |
| --- | --- |
| 3.0 | Keyword arguments separated from positional hashes; `case/in` pattern matching; endless methods `def sq(x) = x * x`; Ractor (experimental); fiber scheduler interface |
| 3.1 | Hash shorthand `{x:, y:}`; anonymous block forwarding `def f(&) = g(&)`; YJIT; Psych 4, so `YAML.load` is safe by default |
| 3.2 | `Data.define`; anonymous `*` and `**` forwarding; `Struct.new` accepts keyword arguments without `keyword_init: true`; `Fiber[]` storage |
| 3.3 | Prism parser shipped; YJIT speedups; M:N thread scheduler (opt-in) |
| 3.4 | `it` as the implicit block parameter; Prism is the default parser; string literals are "chilled" (mutation warns under `-W:deprecated`); `Hash#inspect` prints `{name: "x"}` |

**Keyword argument separation (3.0).** A hash passed positionally no longer becomes keywords, and keywords no longer collapse into a trailing positional hash. Code that relied on the 2.x conversion raises `ArgumentError`. Splat explicitly, and delegate with `...`.

```ruby
def connect(host, port: 5432, ssl: false) = open_socket(host, port, ssl)

opts = { port: 6543, ssl: true }
connect("db", opts)       # ArgumentError on 3.x: opts is a second positional argument
connect("db", **opts)     # keywords

def with_logging(...) = connect(...)   # forwards positionals, keywords and the block
```

**Pattern matching.** `case/in` destructures arrays, hashes and any object implementing `deconstruct`/`deconstruct_keys`. With no matching branch and no `else` it raises `NoMatchingPatternError`, so an unhandled shape fails loudly. `expr => pattern` binds or raises; `expr in pattern` returns a boolean.

```ruby
case response
in { status: 200, body: { items: [_, *] => items } }
  process(items)
in { status: 404 }
  nil
in { status: 500.. => code }
  raise UpstreamError, "upstream returned #{code}"
end

config => { database: { host: String => host } }   # binds host; a missing key raises NoMatchingPatternKeyError, a non-String host NoMatchingPatternError
```

Hash patterns match a subset of keys (`**nil` forbids extra keys); array patterns match the whole array unless a `*rest` is present. Pin a local with `^expected`, an expression with `^(expr)`.

**Value objects with `Data.define` (3.2).** Immutable, keyword or positional construction, value equality, `with` for modified copies, pattern-matchable. Use it where older code used `Struct` for values that should never change; keep `Struct` for the rare case where mutability is the point.

```ruby
Point = Data.define(:x, :y) do
  def +(other) = with(x: x + other.x, y: y + other.y)
end

origin = Point.new(x: 0, y: 0)
moved = origin.with(y: 5)    # origin is unchanged
Point.new(1)                 # ArgumentError: missing keyword: :y
```

**Frozen string literals.** Put `# frozen_string_literal: true` at the top of every file (RuboCop enforces it): literals are frozen and deduplicated, which saves allocations and turns accidental mutation into `FrozenError`. Build strings from `+""` or `String.new(capacity: n)`; `str.dup` returns an unfrozen copy.

## Blocks, Procs and Lambdas

| | Block / `proc {}` | `lambda {}` / `->(x) {}` |
| --- | --- | --- |
| Arity | lenient: missing arguments are `nil`, extras dropped, a lone array auto-splats | strict: `ArgumentError` on mismatch |
| `return` | returns from the method that defined the block | returns from the lambda |
| `next` | ends this call of the block, with a value | same as `return` |

- A method takes a block implicitly (`yield`, `block_given?`) or explicitly (`&block`, which allocates a `Proc`; the anonymous `&` of 3.1 forwards without naming it).
- Return an `Enumerator` when no block is given, so callers can chain: `return enum_for(__method__) unless block_given?`.
- `&:name` calls `Symbol#to_proc`; `&method(:parse)` passes an existing method.
- Blocks capture local variables by reference: a block stored and called later sees the current value and can reassign it.
- `it` (3.4) and `_1` suit one-line blocks; name the parameter when a block spans lines.
- ⚠ `return` inside a `proc` that outlives its defining method raises `LocalJumpError`. Store callbacks as lambdas.

```ruby
def each_batch(size: 100)
  return enum_for(__method__, size:) unless block_given?

  offset = 0
  loop do
    rows = fetch(offset:, limit: size)
    break if rows.empty?

    yield rows
    offset += size
  end
end

each_batch.with_index { |rows, i| logger.info("batch #{i}: #{rows.size} rows") }

present = ->(value) { value.is_a?(String) && !value.empty? }
names.select(&present)          # a lambda passed as the block
ids = users.map { it.id }       # 3.4
```

## Errors and Exceptions

- Bare `rescue` and `rescue => e` catch `StandardError` only. Never `rescue Exception`: it also swallows `Interrupt`, `SystemExit` and `SignalException`, so Ctrl-C and `exit` stop working.
- Give a library one base error and a hierarchy under it. Callers rescue the base for everything or a subclass for one case.
- Raising inside a `rescue` sets `cause` automatically, so the original stays reachable as `e.cause`.
- Keep the `begin` body to the call that can fail. A method body is an implicit `begin`: `def x ... rescue ... ensure ... end`.
- `ensure` runs on every exit path. A `return` inside `ensure` discards the in-flight exception; never write one.
- `retry` re-runs the `begin` body; always bound it with a counter.
- ⚠ `Timeout.timeout` raises into the block from another thread at an arbitrary point, which can leave connections, locks and files half-updated. Use the library's own timeouts (`Net::HTTP#read_timeout`, driver `connect_timeout`, database `statement_timeout`).

```ruby
module Spool
  class Error < StandardError; end
  class SymlinkError < Error; end
  class FetchError < Error; end
end

def fetch_with_retry(url, attempts: 3)
  tries = 0
  begin
    tries += 1
    http_get(url)
  rescue Net::OpenTimeout, Errno::ECONNRESET
    retry if tries < attempts
    raise Spool::FetchError, "fetch failed after #{tries} attempts: #{url}"   # original is #cause
  end
end
```

## Concurrency

- **The GVL** lets one thread run Ruby code at a time (per Ractor). Threads help IO-bound work because blocking IO and `sleep` release the GVL; they give no speedup for CPU-bound Ruby. For CPU parallelism use processes (Puma workers, Sidekiq processes, `Process.fork`) or a native extension.
- **Puma** runs `workers` (processes) times `threads`. Class-level and global mutable state is shared by every thread in a worker: memoized class instance variables, `@@cache ||= {}`, a constant holding a mutable hash. Guard with `Mutex#synchronize`, use `Concurrent::Map` from concurrent-ruby, or keep the state per request.
- **Fibers:** under a fiber scheduler (the `async` gem, Falcon) stdlib IO, `sleep` and `Net::HTTP` yield instead of blocking, so one thread serves many IO-bound tasks.
- **Ractors** give real parallelism with isolated objects, but remain experimental and most gems are not Ractor-safe. Keep them out of application code.
- **Background jobs** go through ActiveJob backends (Sidekiq, Solid Queue, GoodJob). Arguments must be serializable primitives or GlobalID records, and every backend retries, so jobs must be idempotent.

```ruby
class RateCache
  def initialize
    @lock = Mutex.new
    @rates = {}
  end

  def fetch(currency)
    @lock.synchronize { @rates[currency] ||= load_rate(currency) }   # check and set under one lock
  end
end
```

## Rails Essentials

- **Autoloading (Zeitwerk):** paths must match constant names (`app/services/spool/writer.rb` defines `Spool::Writer`). Never `require` an autoloaded file; `bin/rails zeitwerk:check` validates the tree.
- **N+1 queries:** `includes` lets Rails choose, `preload` runs separate queries, `eager_load` uses one `LEFT JOIN` (needed to filter on the association). `strict_loading` on a model, association or relation raises on lazy loads; the `bullet` gem reports them in development.
- **Large tables:** `find_each` / `in_batches`; `Model.all.each` loads every row into memory.
- **Strong parameters:** `params.expect(user: [:name, :email])` (Rails 8) or `params.require(:user).permit(:name, :email)`. Never `permit!`, and never pass raw `params` to `create`/`update`.
- **Callbacks:** keep them to the model's own invariants (normalization, derived columns). `after_save` runs inside the transaction, so an email or job triggered there can run before the commit or for a row that is rolled back; use `after_commit` / `after_create_commit` or a service object for side effects.
- **Bypasses:** `update_column(s)`, `update_all`, `insert_all` and `delete_all` skip validations and callbacks. `validates :email, uniqueness: true` alone races; back it with a unique index.
- **Transactions:** `ActiveRecord::Base.transaction do ... end` rolls back on any exception; `raise ActiveRecord::Rollback` rolls back without propagating.
- **Secrets:** `bin/rails credentials:edit` (encrypted) or environment variables; never plaintext in the repository.
- **Rails 7.1+:** `normalizes`, `generates_token_for`, `authenticate_by` (timing-safe credential lookup). Rails 8 adds the authentication generator and Solid Queue/Cache/Cable defaults.

```ruby
class Order < ApplicationRecord
  belongs_to :customer
  has_many :line_items, dependent: :destroy

  normalizes :reference, with: ->(ref) { ref.strip.upcase }
  validates :reference, presence: true

  after_create_commit -> { OrderMailer.confirmation(self).deliver_later }

  scope :recent, -> { where(created_at: 7.days.ago..) }
end

Order.recent.includes(:customer, :line_items).find_each do |order|
  export(order)    # no per-row customer or line-item queries
end
```

## Testing

**RSpec.**

```ruby
# spec/spool/writer_spec.rb
RSpec.describe "Spool::Writer" do
  subject(:writer) { Spool::Writer.new(root:) }

  let(:root) { Pathname(Dir.mktmpdir) }

  after { FileUtils.remove_entry(root) }

  it "writes the payload into the spool directory" do
    writer.write("job-1", "payload")
    expect(root.join("job-1").read).to eq("payload")
  end

  it "rejects a symlinked spool directory" do
    target = Pathname(Dir.mktmpdir)
    link = root.join("spool")
    File.symlink(target, link)

    expect { Spool::Writer.new(root: link).write("job-1", "payload") }
      .to raise_error(Spool::SymlinkError, /symlink/)
    expect(target.join("job-1")).not_to exist
  ensure
    FileUtils.remove_entry(target) if target
  end
end
```

- `let` is lazy and memoized per example; `let!` runs before every example. Prefer `let`; a `let!` that nothing reads is hidden setup.
- Verified doubles (`instance_double(Spool::Store, put: true)`) fail when the stubbed method does not exist on the real class or is called with the wrong arity; a plain `double` checks nothing.
- Always name the class in `raise_error`: a bare `raise_error` passes on the `NoMethodError` from a typo.
- `aggregate_failures` reports every failed expectation in an example instead of stopping at the first.
- Run in random order (`config.order = :random` plus `Kernel.srand config.seed`); reproduce with `--seed N`; `rspec --bisect` finds the minimal order-dependent pair.
- `.rspec` holds default flags (`--require spec_helper`). Rails apps use `rails_helper` with `config.use_transactional_fixtures = true`.

**Minitest.**

```ruby
# test/spool/writer_test.rb
require "test_helper"

class SpoolWriterTest < Minitest::Test
  def setup
    @root = Pathname(Dir.mktmpdir)
  end

  def teardown
    FileUtils.remove_entry(@root)
  end

  def test_writes_payload_into_spool_directory
    Spool::Writer.new(root: @root).write("job-1", "payload")
    assert_equal "payload", @root.join("job-1").read
  end
end
```

- `assert_equal expected, actual`: expected first, or the failure message reads backwards. Use `assert_nil` for nil; `assert_equal nil, x` is deprecated.
- `assert_raises(Spool::SymlinkError) { ... }` returns the exception, so assert on its message afterwards.
- `minitest/mock`: `Object#stub` replaces a method for the block; `Minitest::Mock#expect` plus `verify` checks calls.
- Minitest randomizes order by default; `--seed N` reproduces a run.
- Rails: `ActiveSupport::TestCase` with `test "writes the payload" do` (defines `test_writes_the_payload`), YAML fixtures in `test/fixtures`, `parallelize(workers: :number_of_processors)`, `bin/rails test test/models/order_test.rb:12`.

## Loom Test Runner Adapter

**Adapter.** Two adapters cover Ruby; `loom project detect` prints the one chosen for each package (kind `ruby`, marker `Gemfile`):

- `rspec` when `.rspec` exists or `Gemfile` names `rspec`;
- `minitest` otherwise.

**Single-test command.** Loom runs it with the package directory as cwd:

```bash
# rspec, when a Gemfile exists
bundle exec rspec {file} -e '{test}'
# rspec, without a Gemfile
rspec {file} -e '{test}'
# minitest
ruby -Itest {file} -n '/^{test}$/'
```

**The `test` field.**

- `rspec`: the example's full description, the group descriptions and the example description joined by spaces. For the spec above: `Spool::Writer rejects a symlinked spool directory`. When the parent group describes a class (`RSpec.describe Spool::Writer`), a nested description starting with `#`, `.` or `::` joins without the space. Copy the value from the runner's output: the failed-examples list prints it after `#`.
- `minitest`: the method name, e.g. `test_rejects_symlinked_spool_directory`.

**No-match behaviour.**

- `minitest` exits 0 when `-n` matches nothing (captured fixture: `0 runs, 0 assertions, 0 failures, 0 errors, 0 skips`).
- `rspec` is documented: no fixture was captured, and loom's parser was written from RSpec's documented output (`N examples, M failures`). An `-e` that matches nothing reports `0 examples, 0 failures` and, by default, exits 0.

Loom reads the runner's summary, so zero executed tests is `NotSelected` whatever the exit code: a contract whose `test` value does not match fails the freeze ("the runner did not select the test").

**Writing contract tests.**

- Keep RSpec contracts at `spec/**/*_spec.rb` and Minitest contracts at `test/**/*_test.rb`, the conventional locations. Loom maps a file to its language profile by extension and test-file glob.
- `rspec -e` selects every example whose full description *contains* the value (RSpec escapes it, so punctuation is literal). Make the contract's description unique in the file and not a substring of another example's ("rejects a symlink" also selects "rejects a symlink loop").
- `minitest` anchors the value as a regular expression. Write contracts as plain `def test_snake_case` methods: minitest/spec `it "..."` blocks compile to `test_0001_...` names whose number shifts when an example is added above, and Rails `test "..."` names keep punctuation the regex would interpret.
- Loom single-quotes the value on the command line; keep single quotes out of contract names.
- ⚠ A spec file that raises while loading (a `require` of a file that does not exist yet, `RSpec.describe Spool::Writer` before the constant exists) is reported as an error outside of examples with `0 examples`, which can read as a test that was never selected. Describe contract groups with a string and reference new constants inside the example, so a missing class raises in the example and counts as a failure. Apply the same rule to Minitest: name the test class without the new namespace (`SpoolWriterTest`).
- The `minitest` command runs without `bundle exec`: `test/test_helper.rb` must put `lib` on the load path and `require "bundler/setup"` when the tests need locked gems. Rails' `test_helper` does this through `config/environment`.

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: spec/spool/writer_spec.rb
    test: "Spool::Writer rejects a symlinked spool directory"
    scenario: points the spool root at a temp directory through a symlink and calls Writer#write
    rejects: a writer that resolves the symlink and writes job-1 into the link target
```

The Minitest form of the same contract uses `file: test/spool/writer_test.rb` and `test: test_rejects_symlinked_spool_directory`. Quote YAML values that contain `: ` or ` #`.

**Build failures.** Ruby has no compile step, so a contract never reports `BuildFailed`; it must fail inside the test at freeze time (see the load-error rule above). A Minitest file that cannot load prints no summary: loom treats the run as unparsed and requires a non-zero exit.

For test-impact runs loom builds one command over several files: `rspec f1 f2`, or `ruby -Itest` per file joined by `&&`.

## Anti-Patterns

```ruby
# ||= memoization recomputes whenever the cached value is nil or false
def admin?
  @admin ||= role_lookup == :admin          # BAD: re-queries on every call for non-admins
end

def admin?
  return @admin if defined?(@admin)         # GOOD: caches false and nil too
  @admin = role_lookup == :admin
end

# Shared default objects
Hash.new([])                                 # BAD: every missing key shares ONE array
Hash.new { |hash, key| hash[key] = [] }      # GOOD: a new array per key
Array.new(3, [])                             # BAD: three references to one array
Array.new(3) { [] }                          # GOOD

# Mutable constants are shared by every caller and every thread
DEFAULTS = { retries: 3 }                    # BAD: one DEFAULTS.merge!(x) changes it everywhere
DEFAULTS = { retries: 3 }.freeze             # GOOD: merge! raises FrozenError; merge copies
```

Quick swaps:

- `rescue Exception` ⇒ `rescue StandardError` or the specific class.
- `send(params[:op])` ⇒ `public_send` against an allowlist of method names.
- `method_missing` alone ⇒ pair it with `respond_to_missing?`, or define the methods with `define_method`.
- `x = a or b` (assigns `a`; `or`/`and` bind looser than `=`) ⇒ `x = a || b`.
- `where("name = '#{name}'")` ⇒ `where(name:)` or `where("name = ?", name)`.
- `Time.now` in Rails ⇒ `Time.current` (respects `config.time_zone`).
- `unless ... else` ⇒ `if` with the branches swapped.
- Reopening core classes globally ⇒ a refinement (`using`) or a wrapper object.
- Leftover `binding.irb`, `debugger`, `puts` debugging ⇒ remove before commit (RuboCop `Lint/Debugger`).
- Float for money ⇒ integer cents or `BigDecimal`.

Ruby evaluates default arguments on every call, so Python's mutable-default trap does not exist; the shared-object traps above are the Ruby equivalent.

## Expert Practices

### Idioms

- `Hash#fetch(:key)` for required keys (`KeyError` names the key); `[]` returns `nil` and moves the failure somewhere else. `dig` for optional nested reads.
- `Integer("42")` raises on garbage; `"abc".to_i` returns `0` silently. `Integer(value, exception: false)` returns `nil` instead.
- Include `Comparable` with `<=>`, or `Enumerable` with `each`, to get the whole protocol.
- Define `eql?` and `hash` together (or use `Data`) for objects used as Hash keys.
- `filter_map`, `each_with_object`, `tally`, `sum`, `partition`, `each_slice`, `zip`, `group_by`, `min_by`: reach for them before a manual accumulator loop.
- `Enumerator::Lazy` (`each_line.lazy.select(...).first(10)`) for large or infinite streams.
- `private` does not apply to `def self.x`; use `private_class_method :x` or a `class << self` block.

### Gotchas

- `Thread.current[:key]` is fiber-local. Under a fiber scheduler each fiber sees its own value; `Thread.current.thread_variable_get` is thread-wide, and `Fiber[:key]` (3.2) is inherited by child fibers.
- `freeze` is shallow: a frozen array's elements stay mutable. Deep-freeze constants built from nested literals, or build them from `Data` values.
- Integer division truncates: `7 / 2 == 3`; use `7.fdiv(2)` or a float operand.
- `array += other` inside a loop allocates a new array on every pass; use `concat` or `<<`, or build the result with `flat_map`.
- `super` without parentheses forwards the current method's arguments; `super()` forwards none. Mixing them up passes unexpected arguments to the parent.
- Closures keep captured objects alive: a `define_method` block that captures a large local retains it for the life of the class.

### Performance

- Enable YJIT in production (`RUBY_YJIT_ENABLE=1` or `--yjit`); Rails 7.2+ turns it on by default when running Ruby 3.3+.
- ActiveRecord: `pluck(:id)` or `select(:id, :name)` instead of loading full models; `exists?` over `present?` on a relation (`present?` loads every row); `size` on a loaded association avoids a `COUNT` query, `count` always issues one.
- Build strings with `<<` on a mutable buffer; `+=` allocates a new string each iteration.
- Profile before tuning: `stackprof` or `vernier` for CPU, `memory_profiler` for allocations, `benchmark-ips` for micro-benchmarks, `rack-mini-profiler` for requests.

### Security

- `Marshal.load` and `YAML.unsafe_load` on untrusted bytes are remote code execution. `YAML.load` is safe from Psych 4 (Ruby 3.1) on; pass `permitted_classes:` explicitly when you need more than primitives.
- `Kernel#open("|cmd")` runs a shell command. Use `File.open` or `URI.open` for the specific need.
- Shell out with an argument list: `system("tar", "-xf", path)` and `Open3.capture3("git", "log", ref)` bypass the shell; the single-string forms go through `/bin/sh`.
- `constantize`, `send`, `public_send` and `order(params[:sort])` on user input need an allowlist.
- Views: ERB escapes output by default; `raw`, `html_safe` and `<%==` bypass it. Use `sanitize` for user HTML.
- Compare secrets with `ActiveSupport::SecurityUtils.secure_compare` or `Rack::Utils.secure_compare`; generate them with `SecureRandom`.
- User-supplied paths: `File.expand_path(path, root)` then check the result starts with the root; `Pathname#realpath` resolves symlinks before the check.
- Run `brakeman` and `bundler-audit` in CI.

## Verification Checklists

**Before marking Ruby work done:**

- [ ] `bundle exec rubocop` (or `standardrb`) clean; every disable is scoped to a range and names the cop
- [ ] Type checker clean where the project uses one (`srb tc`, `steep check`)
- [ ] `bundle exec rspec` / `bin/rails test` green; new behaviour has tests with named error classes in `raise_error`/`assert_raises`
- [ ] `# frozen_string_literal: true` in new files; no `rescue Exception`; no bare `raise_error`
- [ ] Keyword arguments passed explicitly (`**opts`); delegation uses `...`
- [ ] Dependencies added with `bundle add`, never by hand-editing `Gemfile.lock`
- [ ] No `Marshal.load`/`YAML.unsafe_load` on external data; shell-outs use argument lists
- [ ] No string-interpolated SQL; strong parameters on every write path

**Rails review:**

- [ ] No N+1 on new list pages or jobs (`includes`/`preload`, or `strict_loading` passes)
- [ ] Side effects run from `after_commit` (`after_save` runs inside the transaction); jobs idempotent with serializable arguments
- [ ] Uniqueness validations backed by a unique index; bulk writes that skip validations are deliberate
- [ ] `bin/rails zeitwerk:check` passes; `brakeman` reports nothing new

**Contract tests for loom:**

- [ ] `test` value copied from the runner's output (RSpec full description or Minitest method name), unique in its file, no single quotes
- [ ] Contract spec groups described by string; new constants referenced only inside examples
- [ ] The contract fails before the implementation for the reason its `rejects` field names
