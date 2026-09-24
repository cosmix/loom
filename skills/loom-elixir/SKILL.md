---
name: loom-elixir
description: Elixir language expertise for idiomatic, production-quality code.
triggers:
  - elixir
  - ex
  - exs
  - mix
  - hex
  - otp
  - erlang
  - beam
  - genserver
  - supervisor
  - phoenix
  - liveview
  - ecto
  - exunit
  - credo
  - dialyzer
  - oban
  - plug
  - mox
---

# Elixir Language Expertise

## Overview

Idiomatic, production-grade Elixir on the BEAM: mix, pattern matching, processes and OTP (GenServer, supervisors), Phoenix and Ecto, and ExUnit. Assumes competence; the value is in what is easy to get wrong (atom exhaustion, a single GenServer serializing the hot path, unbounded mailboxes, compile-time config baked into releases, LiveView's double mount, Ecto's absent lazy loading) and in writing tests that loom's `mix-test` adapter can select one at a time.

## Tooling

**mix** builds, tests and manages dependencies; Hex is the package registry.

| Task | Command |
| --- | --- |
| New project | `mix new app --sup` (with a supervision tree), `mix phx.new app` (Phoenix) |
| Add a dependency | add it to `deps/0` in `mix.exs` (mix has no `add` command), then `mix deps.get` to resolve and write `mix.lock` |
| Update one dependency | `mix deps.update jason` |
| Remove unused lock entries | `mix deps.unlock --unused`; CI check: `mix deps.unlock --check-unused` |
| Outdated / retired packages | `mix hex.outdated`, `mix hex.audit`; `mix deps.audit` with `mix_audit` for CVEs |
| Compile strictly | `mix compile --warnings-as-errors` |
| Find recompilation chains | `mix xref graph --label compile-connected` |
| Format | `mix format` (config in `.formatter.exs`); CI: `mix format --check-formatted` |
| Lint | `mix credo --strict` |
| Types | `mix dialyzer` (dialyxir); recent compilers also emit gradual type-check warnings |
| Security (Phoenix) | `mix sobelow` |

Commit `mix.lock`. Dependencies with the `only: [:dev, :test]` and `runtime: false` options stay out of releases.

**The gate:**

```bash
mix format --check-formatted
mix compile --warnings-as-errors
mix credo --strict
mix test --warnings-as-errors
mix dialyzer                    # when the project uses dialyxir
```

## Pattern Matching and Control Flow

- `=` matches; it binds on success and raises `MatchError` on failure. Pin an existing value with `^`: `^expected = actual`.
- Prefer multiple function clauses with guards over `if`/`cond` chains inside one body; clause order is match order.
- Return tagged tuples (`{:ok, value}`, `{:error, reason}`) from functions that can fail; the bang variant (`File.read!`) raises instead. Offer both when callers need both.
- `with` chains the happy path. Without `else`, the first non-matching value is returned as is. With `else`, every non-matching shape must have a clause, or `with` raises `WithClauseError`.
- Map patterns match a subset of keys: `%{} = value` matches any map, so test emptiness with `map_size(m) == 0`. A struct pattern `%User{}` also asserts the struct type.
- Binary patterns parse protocols and file headers without manual slicing.

```elixir
defmodule Spool.Writer do
  @spec write(Path.t(), String.t(), iodata()) :: :ok | {:error, :symlink | File.posix()}
  def write(root, job_id, payload) when is_binary(job_id) do
    with {:ok, %File.Stat{type: :directory}} <- File.lstat(root),
         :ok <- File.write(Path.join(root, job_id), payload, [:exclusive]) do
      :ok
    else
      {:ok, %File.Stat{type: :symlink}} -> {:error, :symlink}
      {:ok, %File.Stat{}} -> {:error, :enotdir}
      {:error, reason} -> {:error, reason}
    end
  end
end

<<magic::binary-size(4), version::unsigned-big-16, rest::binary>> = header
```

**Atoms are never garbage-collected** and the atom table has a fixed limit (1,048,576 by default); filling it crashes the VM. Never call `String.to_atom/1` or `:erlang.binary_to_term/1` on external input. Use `String.to_existing_atom/1` or an explicit map from strings to atoms.

**Structs and data:** `@enforce_keys` for required fields; `Keyword.validate!/2` for option lists; `get_in`/`put_in`/`update_in` for nested updates; `Map.update!/3` when the key must exist.

## Processes and OTP

- Processes share nothing and communicate by message; each is cheap (a few KB). Every message is copied into the receiver's heap, except binaries over 64 bytes, which are reference-counted.
- **GenServer** state lives in one process, so every call is serialized. Use a GenServer for state, concurrency control or lifecycle, and plain modules for code organization. A single GenServer on the hot path becomes the bottleneck; serve read-heavy shared state from ETS (`read_concurrency: true`) or `:persistent_term` (rarely changed: every update triggers a global GC pass).
- Do slow initialization in `handle_continue/2` so `start_link` returns and the supervisor is not blocked.
- `GenServer.call` times out after 5_000 ms by default and exits the caller. `cast` has no backpressure: a fast producer grows the receiver's mailbox without bound. Prefer `call` when the sender can outpace the receiver.
- Defining any `handle_info/2` clause replaces the default, which logs unexpected messages; add a catch-all clause, or a stray message crashes the process with `FunctionClauseError`.
- `terminate/2` runs on shutdown only if the process traps exits (`Process.flag(:trap_exit, true)`); never rely on it for durable cleanup.
- Links propagate crashes; monitors deliver a `:DOWN` message. Link what should die together; monitor what you only observe.
- Name dynamic processes through `Registry` (`{:via, Registry, {Spool.Registry, key}}`); never generate atoms for names.

```elixir
defmodule Spool.Queue do
  use GenServer
  require Logger

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  def push(job), do: GenServer.call(__MODULE__, {:push, job})
  def depth, do: GenServer.call(__MODULE__, :depth)

  @impl true
  def init(opts), do: {:ok, %{jobs: :queue.new(), dir: Keyword.fetch!(opts, :dir)}, {:continue, :load}}

  @impl true
  def handle_continue(:load, state), do: {:noreply, %{state | jobs: load_pending(state.dir)}}

  @impl true
  def handle_call({:push, job}, _from, state), do: {:reply, :ok, %{state | jobs: :queue.in(job, state.jobs)}}
  def handle_call(:depth, _from, state), do: {:reply, :queue.len(state.jobs), state}

  @impl true
  def handle_info(msg, state) do
    Logger.warning("unexpected message: #{inspect(msg)}")
    {:noreply, state}
  end
end
```

**Supervisors.** Strategies: `:one_for_one` (restart the failed child), `:rest_for_one` (restart it and every child started after it), `:one_for_all`. Children start in list order and stop in reverse. The default restart intensity is 3 restarts in 5 seconds; past it the supervisor exits and its parent decides. Use `restart: :transient` for children that may finish normally, `DynamicSupervisor` for children started on demand.

```elixir
children = [
  {Registry, keys: :unique, name: Spool.Registry},
  {Task.Supervisor, name: Spool.TaskSupervisor},
  {Spool.Queue, dir: Application.fetch_env!(:spool, :dir)}
]

Supervisor.start_link(children, strategy: :rest_for_one, name: Spool.Supervisor)
```

**Tasks.** `Task.async_stream(items, &fetch/1, max_concurrency: 8, timeout: 10_000)` bounds concurrency; `Task.Supervisor.async_nolink/2` runs work that may crash without taking the caller down. A bare `Task.async` links to the caller, so its crash kills the caller too.

## Phoenix Essentials

- **Contexts** (`Accounts`, `Billing`) own their schemas and `Repo` calls; controllers and LiveViews call contexts, never `Repo` directly.
- **Plug:** `conn` is immutable; each plug returns a new one. A plug that sends a response must `halt/1`, or later plugs keep running.
- **LiveView** `mount/3` runs twice: once for the disconnected HTTP render, once over the WebSocket. Subscribe to PubSub and start timers only when `connected?(socket)`. Use `stream/3` for large collections so the socket does not hold them, and `assign_async/3` or `start_async/3` for slow loads.
- **Verified routes** (`~p"/orders/#{order}"`) are checked at compile time.
- **Config:** `config/config.exs` and `config/prod.exs` are evaluated at build time and baked into the release; read secrets and environment in `config/runtime.exs` with `System.fetch_env!/1`. Inside modules, `Application.compile_env/3` for build-time values (mix tracks them), `Application.get_env/3` at runtime.
- **Security:** HEEx escapes output; `raw/1` bypasses it. Forms carry CSRF tokens by default; keep `:protect_from_forgery` and `:put_secure_browser_headers` in the browser pipeline.

```elixir
def mount(_params, _session, socket) do
  if connected?(socket), do: Phoenix.PubSub.subscribe(Shop.PubSub, "orders")
  {:ok, stream(socket, :orders, Orders.list_recent())}
end

def handle_info({:order_created, order}, socket) do
  {:noreply, stream_insert(socket, :orders, order, at: 0)}
end
```

## Ecto Essentials

- **Changesets:** the field list passed to `cast/4` is the mass-assignment allowlist. `unique_constraint/2` turns the database's unique-index violation into a changeset error and needs that index to exist.
- **No lazy loading:** an association that was not preloaded is `%Ecto.Association.NotLoaded{}`. Preload in the query or with `Repo.preload/2`; preloading inside a loop is N+1.
- **Queries** compose, and `^` interpolates a bound parameter, so pinned values cannot inject SQL. `fragment/1` takes `?` placeholders and a literal string.
- **Transactions:** `Repo.transaction/2` or `Ecto.Multi` for named steps whose failure you want to inspect.
- **Bulk operations:** `Repo.insert_all/3` and `update_all/3` skip changesets, validations and automatic timestamps.
- **Migrations:** `create index(..., concurrently: true)` needs `@disable_ddl_transaction true` and `@disable_migration_lock true`.

```elixir
def changeset(user, attrs) do
  user
  |> cast(attrs, [:email, :name])        # role is never cast from user input
  |> validate_required([:email])
  |> update_change(:email, &String.downcase/1)
  |> unique_constraint(:email)
end

Order
|> where([o], o.customer_id == ^customer_id)
|> preload(:lines)
|> Repo.all()
```

## Background Jobs and Releases

- **Oban** stores jobs in Postgres. Insert the job in the same transaction as the data it processes (`Ecto.Multi` plus `Oban.insert/3`), so a rollback never leaves an orphan job.
- `perform/1` returns `:ok`, `{:ok, value}`, `{:error, reason}` (retried up to `max_attempts`), `{:cancel, reason}` or `{:snooze, seconds}`. A raise also counts as a failed attempt.
- Job args round-trip through JSON: a worker receives string keys (`%{"order_id" => id}`), so match on strings, and pass ids instead of structs.
- Workers must be idempotent: every failure is retried. Add `unique: [period: 60]` where duplicates are possible.
- Test with `Oban.Testing` (`perform_job/2`, `assert_enqueued/1`) and `testing: :manual` in `config/test.exs`.

```elixir
defmodule Shop.Workers.SendConfirmation do
  use Oban.Worker, queue: :mailers, max_attempts: 5

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"order_id" => order_id}}) do
    with {:ok, order} <- Shop.Orders.fetch(order_id) do
      Shop.Mailer.deliver_confirmation(order)
    end
  end
end
```

**Releases:** `MIX_ENV=prod mix release` builds a self-contained release; `config/runtime.exs` runs at every boot. Run migrations from a release module (`bin/shop eval "Shop.Release.migrate()"`), since mix is absent in a release.

## Testing

```elixir
# test/spool/writer_test.exs
defmodule Spool.WriterTest do
  use ExUnit.Case, async: true

  @moduletag :tmp_dir

  describe "write/3" do
    test "writes the payload into the spool directory", %{tmp_dir: dir} do
      assert :ok = Spool.Writer.write(dir, "job-1", "payload")
      assert File.read!(Path.join(dir, "job-1")) == "payload"
    end

    test "rejects a symlinked spool directory", %{tmp_dir: dir} do
      target = Path.join(dir, "target")
      link = Path.join(dir, "spool")
      File.mkdir_p!(target)
      File.ln_s!(target, link)

      assert {:error, :symlink} = Spool.Writer.write(link, "job-1", "payload")
      refute File.exists?(Path.join(target, "job-1"))
    end
  end
end
```

- `test/test_helper.exs` calls `ExUnit.start()`; test files end in `_test.exs`.
- `async: true` runs the module concurrently with other async modules. Leave it off when tests touch global state: application env, named processes, fixed file-system paths, `:persistent_term`.
- `setup` returns a map or keyword list merged into the context; `setup_all` runs once per module; `on_exit/1` registers cleanup. The `:tmp_dir` tag gives each test a fresh directory in the context.
- `start_supervised!/1` starts a process under the test supervisor and stops it when the test ends; prefer it over `start_link` in tests.
- `assert_receive msg, timeout` (default 100 ms) and `refute_receive` replace `Process.sleep` for asynchronous assertions.
- `assert_raise/2`, `capture_log/1`, `capture_io/1`; `doctest Spool.Writer` runs the `iex>` examples in `@doc`.
- **Mox:** mocks only behaviours. `Mox.defmock(Spool.StoreMock, for: Spool.Store)`, `expect/4` in the test, `setup :verify_on_exit!`, and `Mox.allow/3` when another process calls the mock.
- **Ecto sandbox:** `Ecto.Adapters.SQL.Sandbox` wraps each test in a transaction; `async: true` works with it on Postgres. Processes spawned by the test need `allow/3`, or the test runs in shared mode (not async).
- Useful flags: `mix test --failed`, `--stale`, `path/to/file_test.exs:42`, `--seed 0`, `--max-failures 1`, `--trace`, `--slowest 10`.

## Loom Test Runner Adapter

**Adapter.** `mix-test`, for every package with a `mix.exs` (kind `elixir`); `loom project detect` prints it per package.

**Single-test command.** Loom runs it with the package directory as cwd:

```bash
mix test {file} --only 'test:{test}'
```

**The `test` field.** The ExUnit test name including its `test` prefix: `"test "`, then the `describe` text and a space when the test sits in a `describe` block, then the test's own description. For the test above: `test write/3 rejects a symlinked spool directory`. ExUnit prints this name in failure output as `test write/3 rejects ... (Spool.WriterTest)`.

**No-match behaviour.** Documented: no fixture was captured, and loom's parser was written from mix's documented output (`N tests, F failures`, with `, E excluded` when filters apply). mix documents that a run using `--only` in which no tests run fails. Loom reads the runner's summary, so `0 tests` or every test excluded is `NotSelected` whatever the exit code: a contract whose `test` value does not match fails the freeze ("the runner did not select the test").

**Writing contract tests.**

- Keep contracts in `test/**/*_test.exs`; mix loads only files matching its test pattern. Loom maps a file to its language profile by extension and test-file glob.
- The `test:` filter compares the whole name as a string, with no pattern syntax, so the value must match exactly, `describe` text included. ExUnit rejects two tests with the same name in one module at compile time, and `{file}` narrows the run to one file, so a matching value selects exactly one test.
- Loom single-quotes the value on the command line; keep single quotes out of test descriptions.
- A contract test tagged or excluded in `test_helper.exs` (`ExUnit.start(exclude: [:integration])`) still runs: an `--only` filter re-includes the tests it names.

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: test/spool/writer_test.exs
    test: "test write/3 rejects a symlinked spool directory"
    scenario: creates a tmp_dir with a target directory and a symlink to it, calls Writer.write/3 on the link
    rejects: a writer that follows the symlink and writes job-1 into the link target
```

Quote the YAML value; descriptions often contain `: ` or ` #`.

**Build failures.** `mix test` compiles the project and then the test files. A contract that does not compile yet counts as red at freeze time: a struct literal (`%Spool.Writer{}`), an `import`, or a macro from a module that does not exist yet fails compilation before any test runs. A remote call to a function that does not exist yet compiles with a warning and fails inside the test (`UndefinedFunctionError`), which is also red. At completion the test must compile and pass.

Loom's test-impact runs cannot select ExUnit tests by file (`select_command` returns none for `mix-test`); the full suite runs in integration-verify.

## Anti-Patterns

```elixir
# O(n) emptiness checks
if length(list) == 0, do: :empty          # BAD: walks the whole list
case list do
  [] -> :empty                            # GOOD: constant time
  [_ | _] -> :non_empty
end

# Appending in a loop is quadratic
Enum.reduce(items, [], fn x, acc -> acc ++ [f(x)] end)       # BAD
items |> Enum.reduce([], fn x, acc -> [f(x) | acc] end) |> Enum.reverse()   # GOOD (or Enum.map)

# Runtime config read at compile time
@timeout Application.get_env(:spool, :timeout)    # BAD: frozen at build time, untracked
@timeout Application.compile_env(:spool, :timeout, 5_000)  # GOOD when build time is intended
defp timeout, do: Application.get_env(:spool, :timeout, 5_000)  # GOOD for runtime values
```

Quick swaps:

- `String.to_atom(input)` ⇒ `String.to_existing_atom/1` or an explicit mapping.
- `try/rescue` for expected failures ⇒ tagged tuples and `with`.
- Nested `case` pyramids ⇒ `with`, or multi-clause functions.
- `Process.sleep` to wait in tests ⇒ `assert_receive`, or synchronous calls that return when the work is done.
- `import SomeModule` ⇒ `alias`, or `import SomeModule, only: [fun: 1]` (whole-module imports add compile-time dependencies).
- `Enum` over a large file or an unbounded source ⇒ `Stream` (`File.stream!/1` plus `Stream.map`), with one `Enum` call at the end.
- A GenServer per entity for plain data ⇒ a struct and a module of functions.
- `IO.inspect` left in code ⇒ remove, or use `Logger` with metadata.

## Expert Practices

### Idioms

- Start a pipeline with the data, keep each step a single-purpose function, and use `then/2` for a step whose argument is not first.
- `@impl true` on every callback, so a typo in a callback name is a compile warning.
- `@spec` on public functions; behaviours (`@callback`) at every boundary you need to mock or swap.
- Protocols for polymorphism over data types, behaviours for polymorphism over modules.
- `defdelegate` to expose a context's public API from its internal modules.

### Gotchas

- `'abc'` is a charlist; `"abc"` is a binary string. Recent versions print charlists as `~c"abc"`; Erlang functions often want charlists.
- `==` treats `1 == 1.0` as true; `===` does not. Use `===` when integers and floats must stay distinct.
- `and`/`or`/`not` require booleans; `&&`/`||`/`!` accept any truthy value.
- Term ordering sorts structs by their fields: `Enum.sort(dates)` orders `Date` structs wrongly. Pass the module: `Enum.sort(dates, Date)`, `Enum.sort_by(events, & &1.at, DateTime)`.
- Maps with more than 32 keys have no stable iteration order; never depend on it.
- A sub-binary keeps the whole parent binary alive; `:binary.copy/1` a small slice you store long-term from a large message.
- Selective `receive` scans the whole mailbox each time; a large mailbox makes every receive slow.

### Performance

- Measure with `Benchee`; inspect a live node with `:observer`, `:recon`, and `:sys.get_state/1`.
- Keep large data out of messages between processes: every send copies the term.
- ETS for shared reads, `:persistent_term` for rarely-changed global config, `Task.async_stream` with `max_concurrency` for bounded parallel IO.
- Emit `:telemetry` events at boundaries; Phoenix and Ecto already emit them.

### Security

- `:erlang.binary_to_term/2` with `[:safe]` still builds executable funs; use `Plug.Crypto.non_executable_binary_to_term/2` for data that crossed a trust boundary.
- Never `Code.eval_string/1` on input.
- `System.cmd("tar", ["-xf", path])` passes arguments without a shell; `:os.cmd/1` goes through one.
- Compare secrets with `Plug.Crypto.secure_compare/2`; sign tokens with `Phoenix.Token`.
- User-supplied relative paths: `Path.safe_relative/2` (Elixir 1.16+) rejects traversal outside the base.
- Run `mix sobelow` on Phoenix apps and `mix hex.audit` / `mix deps.audit` in CI.

## Verification Checklists

**Before marking Elixir work done:**

- [ ] `mix format --check-formatted` clean
- [ ] `mix compile --warnings-as-errors` and `mix test --warnings-as-errors` pass
- [ ] `mix credo --strict` clean; `mix dialyzer` clean where the project uses it
- [ ] Public functions have `@spec`; callbacks have `@impl true`
- [ ] No `String.to_atom/1` or `binary_to_term` on external data
- [ ] Failure paths return tagged tuples; `with` `else` clauses cover every non-matching shape
- [ ] Dependencies declared in `mix.exs` and resolved with `mix deps.get`; `mix.lock` committed; `mix deps.unlock --check-unused` clean

**OTP and Phoenix review:**

- [ ] No GenServer on a hot read path that ETS or `:persistent_term` could serve
- [ ] Senders that can outpace a receiver use `call`; `handle_info/2` has a catch-all
- [ ] Supervision order and strategy match the dependencies between children
- [ ] Secrets and environment read in `config/runtime.exs`; LiveView subscriptions guarded by `connected?/1`
- [ ] Ecto associations preloaded explicitly; `cast/4` field lists exclude privileged fields

**Contract tests for loom:**

- [ ] `test` value is the full ExUnit name, `test ` prefix and `describe` text included, with no single quotes
- [ ] Contract file ends in `_test.exs` under `test/`
- [ ] The contract fails (or fails to compile) before the implementation for the reason its `rejects` field names
