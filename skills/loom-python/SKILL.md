---
name: loom-python
description: Python language expertise for idiomatic, production-quality code. Use for web frameworks (FastAPI, Django, Flask), data processing (pandas, numpy), ML patterns (sklearn, pytorch), async programming, type hints, pytest, packaging with uv/poetry, and linting with ruff/mypy/black.
triggers:
  - python
  - py
  - pip
  - uv
  - poetry
  - virtualenv
  - pytest
  - pydantic
  - fastapi
  - django
  - flask
  - pandas
  - numpy
  - dataclass
  - type hints
  - asyncio
  - mypy
  - ruff
  - black
  - sklearn
  - pytorch
  - tensorflow
  - jupyter
  - pipenv
  - conda
---

# Python Language Expertise

## Overview

Idiomatic, production-grade Python: modern typing, async, packaging, and the web/data/ML ecosystem. Assumes competence — the value is in what is easy to get wrong (interning, mutable defaults, blocking the event loop, GIL, dataclass/Pydantic boundaries), not syntax tutorials.

## Tooling (uv / ruff / mypy)

Modern Python has consolidated onto two Rust tools. Reach for them by default.

**`uv`** — one binary replacing pip + pip-tools + virtualenv + pyenv + pipx + (mostly) poetry; resolves/installs 10–100× faster.

| Task | Command |
| --- | --- |
| New project | `uv init` (writes `pyproject.toml`) |
| Add / remove dep | `uv add httpx` / `uv remove httpx` (updates `pyproject.toml` + `uv.lock`) |
| Add dev dep | `uv add --dev pytest` |
| Sync env from lock | `uv sync` (creates `.venv`, exact/reproducible) |
| Run in project env | `uv run pytest` (auto-syncs first; no manual activate) |
| Install a Python | `uv python install 3.12` |
| Ephemeral tool | `uvx ruff check` (= `uv tool run`) |
| pip shim | `uv pip install ...` (drop-in, no project file) |

- `uv add` edits the manifest for you — never hand-edit `[project.dependencies]`. `uv.lock` is the committed lockfile; `uv sync --frozen` in CI to fail on drift.

**`ruff`** — single linter+formatter replacing flake8, isort, pyupgrade, pydocstyle, autoflake, and much of bandit.

- `ruff check --fix` (lint+autofix), `ruff format` (near-identical to black, replaces it).
- Config lives under `[tool.ruff.lint]` since 0.1 — top-level `select` is deprecated and warns.
- Useful rule groups: `E`/`W` pycodestyle, `F` pyflakes, `I` isort, `UP` pyupgrade (keeps typing current), `B` bugbear (catches mutable defaults, `lru_cache` on methods), `SIM`, `S` bandit-security, `ASYNC`, `PL` pylint, `RUF`. Suppress with `# noqa: E501` (never bare `# noqa`).

**`mypy --strict`** for type checking; `pyright`/`basedpyright` (what Pylance runs) is faster and stricter on inference. Always scope ignores: `# type: ignore[arg-type]`, never bare `# type: ignore`.

## Type Hints

- 3.9+: built-in generics `list[T]`, `dict[K, V]`. 3.10+: `X | Y`, `X | None` (not `Union`/`Optional`).
- Import `Callable`/`Sequence`/`Mapping`/`Iterator` from `collections.abc`, not `typing`.
- **Accept abstract, return concrete:** take `Iterable`/`Sequence`/`Mapping` (max caller flexibility); return `list`/`dict` (callers know exactly what they get).
- `Self` (3.11+) for fluent builders / alternate constructors. `TypedDict` for structured dicts (JSON payloads, `**kwargs`), `Protocol` for structural typing, `ParamSpec` for signature-preserving decorators — see Expert Practices.

```python
from collections.abc import Iterable
from typing import TypedDict, NotRequired, Self

class MovieRow(TypedDict):            # dict shape checkable by mypy
    title: str
    year: NotRequired[int]           # 3.11+: optional key

def deduplicate(items: Iterable[str]) -> list[str]:  # abstract in, concrete out
    return list(dict.fromkeys(items))

class Builder:
    def with_name(self, name: str) -> Self:
        self._name = name
        return self
```

## Async / Await

Prefer `asyncio.TaskGroup` (3.11+) over `gather()`: on failure it cancels remaining sibling tasks, awaits them, and re-raises all errors as an `ExceptionGroup` (handled with `except*`). `gather()` does NOT cancel siblings on failure (they "won't be cancelled and will continue to run"), and `return_exceptions=True` silently mixes return values and exception objects into one list. Reserve `gather()` for genuinely independent fire-and-forget results.

⚠ **Never block the event loop.** A coroutine that calls blocking IO (`requests`, sync DB drivers, `open().read()`), `time.sleep()`, or CPU-heavy work freezes *every* task on that loop. Offload: `await asyncio.to_thread(fn, *args)` (3.9+) for blocking IO; `loop.run_in_executor(process_pool, fn)` for CPU work; `await asyncio.sleep()` not `time.sleep()`. See Async Gotchas for the full set.

```python
import asyncio

async def process_batch(urls: list[str]) -> list[dict]:
    async with asyncio.TaskGroup() as tg:
        tasks = [tg.create_task(fetch_data(url)) for url in urls]
    return [t.result() for t in tasks]   # ExceptionGroup raised on any failure

try:
    await process_batch(urls)
except* ConnectionError as eg:           # subgroup handling
    for exc in eg.exceptions:
        logger.error("fetch failed: %s", exc)

rows = await asyncio.to_thread(cursor.execute, sql)   # sync driver off the loop
```

## Dataclasses vs Pydantic (the validation boundary)

**Choose by trust, not habit.**

| | `@dataclass` (stdlib) / `attrs` | Pydantic v2 `BaseModel` |
| --- | --- | --- |
| Validation | **None** — type hints are not enforced at runtime | Validates + coerces on construction |
| Use for | Internal, already-trusted structs; hot paths | Untrusted edges: API bodies, config, external JSON |
| Cost | Free | Per-instance validation (Rust core, but non-zero) |

Don't trust a dataclass with external input (it will happily hold `age="banana"`); don't pay Pydantic's validation cost for internal structs constructed in a tight loop. For perf-critical validated decode, `msgspec` is the fastest option.

```python
from dataclasses import dataclass, field
from pydantic import BaseModel, Field, field_validator

@dataclass(slots=True)               # internal; slots=True saves memory + blocks typos
class Config:
    host: str
    port: int = 8080
    tags: list[str] = field(default_factory=list)   # NEVER tags: list = []

class UserCreate(BaseModel):         # untrusted edge
    email: str = Field(min_length=5)
    name: str = Field(max_length=100)

    @field_validator("email")
    @classmethod
    def norm_email(cls, v: str) -> str:
        if "@" not in v:
            raise ValueError("invalid email")   # NEVER assert (-O strips it)
        return v.lower()
```

⚠ Plain `@dataclass` sets `__hash__ = None` (unhashable — cannot be a set member / dict key). Use `frozen=True` for a hashable value object. `slots=True`/`kw_only=True` are 3.10+.

## Packaging (`pyproject.toml`)

```toml
[project]
name = "mypackage"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["httpx>=0.25", "pydantic>=2.0"]

[dependency-groups]                  # PEP 735; `uv add --dev` targets this
dev = ["pytest>=8", "pytest-asyncio>=0.23", "mypy>=1.9", "ruff>=0.4"]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.ruff.lint]                     # MUST be nested since ruff 0.1; top-level is deprecated
select = ["E4", "E7", "E9", "F", "I", "UP", "B", "SIM"]

[tool.mypy]
strict = true

[tool.pytest.ini_options]
asyncio_mode = "auto"                # else async tests are silently UNCOLLECTED (default "strict")
```

## Web Frameworks

### FastAPI

Type hints drive validation, serialization, and OpenAPI. Use `Annotated[..., Depends(...)]` for injection; `response_model` to shape/filter output.

```python
from typing import Annotated
from fastapi import FastAPI, Depends, HTTPException, status

app = FastAPI()

async def get_db() -> AsyncIterator[AsyncSession]:   # yield-dependency: cleanup after response
    async with AsyncSession(engine) as s:
        yield s

@app.post("/users/", response_model=User, status_code=status.HTTP_201_CREATED)
async def create_user(user: UserCreate, db: Annotated[AsyncSession, Depends(get_db)]) -> User:
    return await UserService(db).create(user)
```

⚠ Gotchas:

- **`def` vs `async def` path ops:** a `def` handler runs in FastAPI's threadpool (blocking libs OK). An `async def` handler runs on the event loop — any blocking call inside stalls *all* requests. Pick `def` for sync ORMs, `async def` only with async libs.
- `response_model` strips fields not on the model — the primary guard against leaking hashes/tokens; don't return the raw ORM object.
- `Depends` results are cached per-request; use `BackgroundTasks` for post-response work.

### Django

```python
from django.db import models, transaction

class User(models.Model):
    email = models.EmailField(unique=True)
    class Meta:
        indexes = [models.Index(fields=["email"])]

with transaction.atomic():                       # rolls back on exception
    user = User.objects.create(email=e)
    Profile.objects.create(user=user)
```

⚠ QuerySets are lazy (evaluated on iteration/`len`/slice). Kill N+1 with `.select_related()` (FK/1:1 join) and `.prefetch_related()` (M2M/reverse). Batch with `bulk_create`/`bulk_update`. `.only()`/`.defer()` to trim columns.

## Data & ML (dense reference)

The APIs are well-known; these are the correctness/perf traps that bite in review.

**pandas**

- **Vectorize.** `df["c"] = df["a"] * df["b"]`, `.map`/`np.where`/masks — never `iterrows`/`itertuples`/row-wise `apply` (Python-level loop, 10–100× slower).
- **Assign via `.loc`**, never chained (`df[m]["c"] = ...`) — raises `SettingWithCopyWarning` and may no-op. pandas 3.0 makes Copy-on-Write the default, ending the ambiguity.
- Memory: `read_csv(usecols=, dtype=)`; nullable `Int64`/`boolean` for NA-bearing ints; `category` dtype for low-cardinality strings; chunk with `read_csv(chunksize=)` or move to `polars`/`dask`/`pyarrow` at scale.
- `merge` silently fans out on non-unique keys (row explosion) — validate with `merge(..., validate="one_to_many")`.

**numpy**

- Slices are **views**, not copies — `b = a[1:3]; b[0] = 9` mutates `a`. `.copy()` to break aliasing.
- Broadcasting + ufuncs over Python loops; watch silent `int` overflow on fixed-width dtypes.
- Use `rng = np.random.default_rng(seed)` (Generator API), not legacy global `np.random.seed`/`np.random.rand`.

**scikit-learn**

- Wrap preprocessing + estimator in a `Pipeline` so scalers/encoders fit on the **train fold only** inside CV — fitting a scaler on the full dataset before splitting leaks test statistics and inflates scores. `ColumnTransformer` for heterogeneous columns. Set `random_state` for reproducibility.

**PyTorch**

- Training step order: `optimizer.zero_grad()` → forward → `loss.backward()` → `optimizer.step()`. Forgetting `zero_grad` accumulates gradients.
- Eval: `model.eval()` (disables dropout, freezes BatchNorm running stats) **and** `torch.inference_mode()` / `torch.no_grad()` (no autograd graph). Both are needed; they do different things.
- Accumulate metrics as `loss.item()`/`.detach()` — keeping tensors alive retains the whole graph → memory blowup. `DataLoader(num_workers>0, pin_memory=True)` for GPU throughput; move batches with `.to(device, non_blocking=True)`.

## Testing with pytest

Async fixtures must use `@pytest_asyncio.fixture` (a plain `@pytest.fixture` on `async def` yields an un-awaited coroutine under strict mode). Session/module-scoped async fixtures need `loop_scope=`; the `event_loop` fixture was removed in pytest-asyncio 1.0.

Built-in fixtures worth reaching for:

- `tmp_path` (per-test `pathlib.Path` dir), `monkeypatch` (`setattr`/`setenv`/`chdir`/`syspath_prepend`, all auto-undone at teardown — never patch globals by hand), `capsys`/`caplog`, `pytest.raises(ValueError, match="regex")`.
- `conftest.py` fixtures are auto-discovered by every test in that directory tree — no import. Fixture `scope=` is `function` (default) / `class` / `module` / `session`; `autouse=True` applies without being requested.
- Run tips: `-x` stop on first fail, `-k EXPR` select, `--lf` last-failed, `-q`, parametrize `ids=` for readable names.

```python
import pytest, pytest_asyncio

@pytest_asyncio.fixture
async def db_session() -> AsyncIterator[AsyncSession]:
    async with AsyncSession(engine) as s:
        yield s

@pytest.mark.parametrize("email,valid", [("a@b.com", True), ("bad", False)])
def test_email(email: str, valid: bool) -> None:
    if valid:
        User(email=email)
    else:
        with pytest.raises(ValueError):
            User(email=email)

def test_env(monkeypatch):
    monkeypatch.setenv("API_KEY", "test")   # auto-restored after the test
    assert load_config().key == "test"
```

## Anti-Patterns

The two that survive linting and cause real bugs:

```python
# Mutable default arg — the list is created ONCE and shared across all calls
def append(item, target=[]):        # BAD
    target.append(item); return target
def append(item, target=None):      # GOOD
    target = [] if target is None else target
    target.append(item); return target

# Late-binding closure — every lambda shares one 'i' cell, read at call time
fns = [lambda: i for i in range(3)]        # BAD -> [2, 2, 2]
fns = [lambda i=i: i for i in range(3)]     # GOOD -> [0, 1, 2] (bind eagerly)
```

Quick swaps (ruff/mypy catch most): `except:` → `except (ValueError, RuntimeError) as e:` (bare except also traps `KeyboardInterrupt`/`SystemExit`); `type(x) == list` → `isinstance(x, list)`; `x == None` → `x is None`; `"a"+str(n)` → `f"a{n}"`; manual `open`/`close` → `with open(...)`; keep `try:` blocks to the single line that can raise so the wrong exception isn't caught.

**f-string debug specifier:** `f"{expr=}"` prints `expr=value` (great for logging); combine with format specs: `f"{ratio=:.2%}"`.

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance for production Python. Each item states the mechanism, not just the rule.

### Concurrency Model & the GIL

Pick the right primitive; the GIL makes the choice non-obvious.

- **CPython has a GIL:** exactly one thread executes Python bytecode at a time. Threads help **IO-bound** work (the GIL is released during blocking IO and in many C extensions), but give **near-zero speedup for CPU-bound** pure-Python work.
- **CPU-bound → processes:** `ProcessPoolExecutor`/`multiprocessing`, or push the loop into NumPy / a C-extension that releases the GIL.
- **IO-bound, many connections → `asyncio`** (single thread, cooperative). **IO-bound with only blocking libraries → threads** (`ThreadPoolExecutor`). asyncio gives no parallelism for CPU work.
- **Free-threaded builds (PEP 703):** 3.13 ships an experimental no-GIL interpreter (`python3.13t`, `--disable-gil`); 3.14 continues it (officially supported but still opt-in, phase II). Not the default, and much of the C-extension ecosystem isn't ready — don't assume it in library code.

```python
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
with ProcessPoolExecutor() as pool:            # CPU-bound: real parallelism
    results = list(pool.map(cpu_heavy, chunks))
with ThreadPoolExecutor(max_workers=32) as pool:  # blocking IO: GIL released during waits
    results = list(pool.map(fetch_url, urls))
```

### Typing Idioms

**Use PEP 695 type-parameter syntax for generics and aliases (3.12+).** Square brackets on a class/def declare type parameters in their own scope — no module-level `TypeVar` or `Generic[T]` base — and the `type` statement defines lazily-evaluated aliases so forward/recursive references work without quoting. Variance is inferred. Keep `TypeVar`/`Generic`/`ParamSpec` only for libraries supporting < 3.12; do not mix old `TypeVar` and new syntax in one class.

```python
class Stack[T]:
    def __init__(self) -> None:
        self._items: list[T] = []

type Vector = list[float]
def max_item[T: (int, float)](items: list[T]) -> T: ...  # inline constraints
```

**Prefer `TypeIs` over `TypeGuard` for narrowing predicates (3.13+, PEP 742).** `TypeGuard` narrows only the `True` branch and permits unsound narrowing to an unrelated type. `TypeIs` narrows in BOTH branches and requires the narrowed type to be compatible with the input — sound for the common `isinstance`-style predicate. Both are in `typing_extensions` for older Pythons.

```python
from typing import TypeIs

def is_str_list(val: list[object]) -> TypeIs[list[str]]:
    return all(isinstance(x, str) for x in val)
# narrows to list[str] in the if-branch AND back to list[object] in else
```

**Use `ParamSpec` + `Concatenate` to keep decorators signature-transparent (PEP 612).** A wrapper typed `(*args: Any, **kwargs: Any) -> Any` erases all parameter info for callers and IDEs. `P.args`/`P.kwargs` capture the full parameter list as a unit so the original signature flows through; pair with a `TypeVar` for the return. `Concatenate[X, P]` models injecting/removing a leading parameter (e.g. supplying a `Session` first).

```python
from functools import wraps
from typing import Callable, ParamSpec, TypeVar
P = ParamSpec("P"); R = TypeVar("R")

def retry(n: int = 3) -> Callable[[Callable[P, R]], Callable[P, R]]:
    def deco(fn: Callable[P, R]) -> Callable[P, R]:
        @wraps(fn)
        def wrap(*a: P.args, **k: P.kwargs) -> R:
            last: Exception | None = None
            for _ in range(n):
                try: return fn(*a, **k)
                except Exception as e: last = e
            raise last
        return wrap
    return deco
```

**Use `Protocol` for structural subtyping, and know `@runtime_checkable`'s limits.** Any class with matching members satisfies a `Protocol` without inheriting, avoiding coupling third-party types to your ABC hierarchy. But `isinstance()` against a runtime-checkable protocol checks only the *presence* of methods/attributes, not signatures or types; since 3.12 it uses `inspect.getattr_static()`, so dynamic `__getattr__`/descriptor-synthesized attributes are no longer seen. The docs note it is "surprisingly slow" — prefer `hasattr()` in hot paths.

```python
from typing import Protocol, runtime_checkable

@runtime_checkable
class Closeable(Protocol):
    def close(self) -> None: ...

class DBConnection:          # satisfies Closeable with no inheritance
    def close(self) -> None: ...
```

**Prefer `__init_subclass__` over a metaclass for subclass registration/validation.** Metaclasses cause metaclass-conflict errors under multiple inheritance. `__init_subclass__` (3.6+) runs on the base whenever a subclass is defined, receives keyword args from the class header, and composes via `super().__init_subclass__(**kwargs)`. Reserve metaclasses for framework work that must alter the class namespace before the body runs.

```python
class PluginBase:
    _registry: dict[str, type] = {}
    def __init_subclass__(cls, plugin_name: str = "", **kwargs):
        super().__init_subclass__(**kwargs)
        if plugin_name:
            PluginBase._registry[plugin_name] = cls

class MyPlugin(PluginBase, plugin_name="my_plugin"): ...
```

### Async Gotchas

**Keep a strong reference to `create_task()` results or the task can be GC'd mid-execution.** The event loop holds only weak references to tasks, so a fire-and-forget task whose return value is discarded "may get garbage collected at any time, even before it's done" — a silent partial no-op. Store each task in a long-lived set and auto-discard via `add_done_callback`.

```python
background_tasks: set[asyncio.Task] = set()
task = asyncio.create_task(some_coro())
background_tasks.add(task)                       # strong reference
task.add_done_callback(background_tasks.discard) # bounded cleanup
```

**Never silently swallow `asyncio.CancelledError`.** Since 3.8 it inherits from `BaseException`, so `except Exception` won't catch it. The real trap is catching it and NOT re-raising: `TaskGroup`/`asyncio.timeout()` cancel children by injecting `CancelledError`, so a coroutine that suppresses it leaves the group or timeout waiting forever.

```python
async def cleanup_handler():
    try:
        await asyncio.sleep(3600)
    except asyncio.CancelledError:
        # do cleanup...
        raise  # MUST re-raise so TaskGroup/timeout can complete
```

**Never block the event loop (expanded).** One coroutine running blocking IO or CPU work halts every other task on the loop — no error, just latency spikes and stalled health checks. Offload blocking IO with `await asyncio.to_thread(fn, *args)` (3.9+); offload CPU work with `loop.run_in_executor(ProcessPoolExecutor(), fn, *args)`. Use async-native libraries (`httpx`/`aiohttp`, `asyncpg`) rather than sync ones inside coroutines.

**Prefer `asyncio.timeout()` (3.11+) over `wait_for()`.** It participates in cooperative cancellation; `wait_for(gather(...))` cancels the gather but the gathered coroutines keep running.

**Use `contextvars.ContextVar`, not `threading.local()`, for per-task state in async code.** asyncio runs all coroutines on one thread, so every coroutine shares the same `threading.local()` namespace — isolation is silently defeated. Each `asyncio.Task` runs in a shallow copy of the current `Context`, so a `ContextVar` set in one task does not leak to siblings (and a child's mutation does not propagate back to the parent).

```python
from contextvars import ContextVar
request_id: ContextVar[str] = ContextVar("request_id")  # module-level

async def handle_request(rid: str):
    token = request_id.set(rid)
    try:
        await do_work()
    finally:
        request_id.reset(token)
```

### Exception Handling

**Understand `except*` / `ExceptionGroup` (3.11, PEP 654) — `TaskGroup` callers must handle one.** Each `except*` clause handles a subgroup, unmatched exceptions are re-raised as a new group, and you cannot mix `except` and `except*` in one `try` (SyntaxError). A single exception raised inside an `except*` block is wrapped into an `ExceptionGroup` — use bare `raise` to preserve provenance. `break`/`continue`/`return` are SyntaxErrors inside `except*`. Use `.split()`/`.subgroup()` with predicates for filtering beyond type.

**A `@contextmanager` that catches an exception without re-raising silently suppresses it.** When a `with` block raises, the exception is thrown into the generator at the `yield`; if the generator catches it and does not re-raise, the `with` statement is told the exception was handled and execution continues — a silent control-flow/data-loss bug. To merely log, you MUST re-raise. (Generators are also single-use: reusing one raises `RuntimeError: generator didn't yield`.)

```python
from contextlib import contextmanager

@contextmanager
def managed_conn(url: str) -> Iterator[Connection]:
    conn = connect(url)
    try:
        yield conn
    except OperationalError:
        logger.error("connection error")
        raise  # MUST re-raise or the exception vanishes
    finally:
        conn.close()
```

**Use `ExitStack` for dynamic/conditional cleanup.** It handles a variable number of resources, conditional cleanup, and rollback during `__enter__` — cases fixed `with`-nesting cannot. `pop_all()` transfers callbacks to a fresh stack (invoking nothing) for "commit on success" ownership transfer; `push()` registers a context manager's `__exit__` and CAN suppress exceptions; `callback()` registers a plain function that CANNOT suppress. Reusable but not reentrant; `AsyncExitStack` exposes `aclose()`.

```python
from contextlib import ExitStack

def open_all(filenames: list[str]):
    with ExitStack() as stack:
        files = [stack.enter_context(open(f)) for f in filenames]
        keep_open = stack.pop_all()   # transfer ownership; stack now empty
    return files, keep_open           # caller closes via keep_open.close()
```

### Data Model Gotchas

**`is` vs `==`, and the interning trap.** Use `is` only for singletons (`None`, `True`, `False`, module-level sentinels) and genuine identity; use `==` for value equality. CPython interns small ints `[-5, 256]` and some short strings, so `a is b` may be `True` for `256` but `False` for `257` — never rely on it for value comparison. Python emits `SyntaxWarning: "is" with a literal` (since 3.8) for `x is 0`. For "absent" markers distinct from `None`, use a unique sentinel object.

```python
_MISSING = object()                  # unique sentinel, compared with `is`
def get(d, key, default=_MISSING):
    v = d.get(key, _MISSING)
    return default if v is _MISSING else v
```

**Return `NotImplemented` (never raise `NotImplementedError`) from arithmetic/comparison dunders.** Returning the `NotImplemented` singleton lets the interpreter try the reflected operation (`other.__radd__`) or fall back to identity comparison; raising `NotImplementedError` short-circuits that chain and breaks interop. As of Python 3.14, evaluating `NotImplemented` in a boolean context raises `TypeError` (previously `True` + `DeprecationWarning` since 3.9), so returning it from a predicate by mistake now fails loudly.

```python
class Vector:
    def __add__(self, other):
        if isinstance(other, Vector):
            return Vector(self.x + other.x, self.y + other.y)
        return NotImplemented  # interpreter then tries other.__radd__(self)

    def __eq__(self, other):
        if not isinstance(other, Vector):
            return NotImplemented
        return (self.x, self.y) == (other.x, other.y)
```

**Data descriptors shadow the instance `__dict__`; non-data descriptors do not.** Lookup order in `object.__getattribute__`: data descriptors (define `__set__`/`__delete__`) > instance `__dict__` > non-data descriptors (`__get__` only) and other class vars > `__getattr__`. This is why plain methods can be shadowed by instance attributes, while `property` and `__slots__` (data descriptors) always win. Use `__set_name__` so a descriptor learns its own attribute name.

```python
class Validated:                 # data descriptor: defines __set__ -> always wins
    def __set_name__(self, owner, name):
        self.private = f"_{name}"
    def __get__(self, obj, objtype=None):
        return self if obj is None else getattr(obj, self.private)
    def __set__(self, obj, value):
        if not isinstance(value, int):
            raise TypeError("must be int")
        setattr(obj, self.private, value)
```

**`__slots__` for memory + attribute discipline.** Declaring `__slots__` drops each instance's `__dict__`, cutting per-instance memory (large for millions of objects) and rejecting typo'd attribute assignment. Cost: no dynamic attributes, and every class in the MRO must define `__slots__` or a `__dict__` reappears. `@dataclass(slots=True)` (3.10+) generates it; note it returns a *new* class object.

### Dataclass Gotchas

Beyond the hashability rule (above):

- **Field ordering is enforced across inheritance:** a field without a default cannot follow one with a default, even across base/subclass boundaries (`TypeError` at class definition). `kw_only=True` sidesteps this.
- **`replace()` does not copy `init=False` fields:** it re-runs `__post_init__`, recomputing them from new values; passing an `init=False` field in the changes is a `ValueError`.
- **Python 3.13 changed generated `__eq__`** from tuple comparison to field-by-field, flipping NaN behavior: `C(float("nan")) == C(float("nan"))` was `True` (tuple identity short-circuit) but is `False` in 3.13+.
- **Frozen `__post_init__` must use `object.__setattr__`** to set computed/`init=False` fields — `self.x = ...` raises `FrozenInstanceError`. `frozen` is not transitive: a frozen subclass of a non-frozen base does not freeze the base's fields.

```python
@dataclass(frozen=True)
class Point:
    x: float
    y: float
    magnitude: float = field(init=False)
    def __post_init__(self) -> None:
        object.__setattr__(self, "magnitude", (self.x**2 + self.y**2) ** 0.5)
```

### Scoping Gotchas

**The walrus `:=` inside a comprehension binds in the ENCLOSING scope (PEP 572).** Unlike the comprehension's own iteration variable (comprehension-local), a name bound with `:=` leaks outside — sometimes intended (capture the last match), often surprising pollution. You cannot rebind the `for`-target with `:=` (SyntaxError), and `:=` in a comprehension whose nearest enclosing scope is a class body raises SyntaxError.

```python
if any((last := item).startswith("x") for item in items):
    print(f"first x-item: {last}")   # intentional capture
results = [f(x) for x in data]       # plain comprehension to avoid leakage
```

### Performance & Caching

**Generators over lists for large/streamed data.** A generator holds one item at a time; a list materializes everything. Prefer `yield`/generator expressions for pipelines and big files; reach for a list only when you need random access, `len`, or multiple passes.

**`functools.cache` for finite-domain pure functions; don't lean on `lru_cache`'s default `maxsize=128`.** `cache` (3.9+) is an unbounded dict-backed memo with no LRU overhead — right for pure functions over a small/finite arg space. `lru_cache`'s default `maxsize=128` is a footgun: callers assume it bounds memory, but 128 may thrash a hot set or mislead. Size it deliberately or use `cache`. Both expose `.cache_info()`/`.cache_clear()`; `__wrapped__` bypasses the cache for testing.

**`lru_cache`/`cache` on an instance method leaks the instance (Ruff B019).** `self` becomes part of the class-level cache key, so the cache keeps a strong reference to every instance forever, defeating GC. Use `functools.cached_property` for zero-arg computed attributes (freed with the instance), or move the computation to a module-level `@lru_cache` function taking only hashable primitives.

```python
from functools import cached_property, cache

class Foo:
    def __init__(self, data: list[int]):
        self.data = data
    @cached_property            # per-instance; freed with the instance
    def expensive_result(self) -> int:
        return sum(self.data)

@cache                          # pure function over a finite domain
def fib(n: int) -> int:
    return n if n < 2 else fib(n - 1) + fib(n - 2)
```

**`multiprocessing` `fork()` with live threads deadlocks; use `spawn` or `forkserver`.** POSIX `fork()` copies only the calling thread but duplicates all lock state, so the child can inherit locks (logging, NumPy BLAS, connection pools) held forever with no thread to release them. Python 3.12 warns on `os.fork()` in a multi-threaded process; 3.14 changed the POSIX default from `fork` to `forkserver`. Set the method explicitly.

```python
import multiprocessing as mp
if __name__ == "__main__":
    mp.set_start_method("spawn")   # no thread/lock inheritance
    with mp.Pool(4) as pool:
        results = pool.map(worker_fn, data)
```

### Library Gotchas

**`logging.config.dictConfig`/`fileConfig` disable existing loggers by default.** Both default `disable_existing_loggers` to `True`, so any logger created before the config call (the typical module-level `getLogger(__name__)`) is silently disabled — a frequent cause of mysterious log silence. Always pass `disable_existing_loggers=False`. Related: `basicConfig()` is a no-op if the root logger already has handlers, so libraries should add only a `NullHandler` and leave configuration to the application.

**Pydantic v2 `model_validator(mode='before')` input may be a non-dict; validators skip defaults; never `assert`.** A before-validator receives the raw input — "which can be anything"; with `from_attributes` it can be an arbitrary object, so `data["x"]` raises. Narrow with `isinstance()` first. Validators do not run on default values unless `Field(validate_default=True)`. Never validate with `assert` — `-O` strips asserts; raise `ValueError`.

```python
@model_validator(mode="before")
@classmethod
def normalize(cls, data: Any) -> Any:
    if isinstance(data, dict):
        data["name"] = data.get("name", "").strip()
    return data  # pass non-dict inputs through untouched
```

### Currency

**`typing.TypeAlias` and `typing.AnyStr` are deprecated.** `TypeAlias` is deprecated since 3.12 in favor of the `type` statement (lazily-evaluated, native forward references). `AnyStr` is deprecated since 3.13 — replace with a PEP 695 constrained type parameter `def f[T: (str, bytes)]` on 3.12+, or `TypeVar("S", str, bytes)` on older Pythons.

**Avoid `from __future__ import annotations` in runtime-introspective code.** PEP 563 stringifies all annotations, breaking libraries that read `__annotations__` or call `typing.get_type_hints()` at runtime: re-evaluating the strings fails when referenced types live under `TYPE_CHECKING` guards or local scopes (historic source of dataclasses `ClassVar`/`InitVar` bugs). PEP 563 was superseded by PEP 649 (deferred annotations) in 3.14; PEP 749 schedules the future-import for eventual deprecation. Quote only the forward references that genuinely need it.

```python
class Node:
    def children(self) -> list["Node"]: ...  # quote only the real forward ref
```

## Verification Checklists

**Before marking Python work done:**

- [ ] `ruff check --fix` clean and `ruff format` applied (or project equivalent)
- [ ] `mypy --strict` (or configured strictness) passes; every `# type: ignore` has a `[code]`
- [ ] `pytest` green; new behavior has tests (parametrized edge cases, `pytest.raises(match=)`)
- [ ] No mutable default args; no bare `except:`; no `assert` used for validation
- [ ] Public functions type-hinted (abstract params in, concrete return); no unused imports/vars
- [ ] Untrusted input goes through Pydantic/validation, not a bare dataclass
- [ ] No hardcoded secrets; f-strings not used to build SQL/shell (parameterize / `shlex`)
- [ ] Deps added via `uv add`/`cargo`-style tool, not hand-edited `pyproject.toml`

**Async / concurrency review:**

- [ ] No blocking IO, `time.sleep`, or CPU work inside `async def` — offloaded via `to_thread`/executor
- [ ] `create_task` results kept referenced; `CancelledError` re-raised, not swallowed
- [ ] `TaskGroup` (not `gather`) for fail-fast; callers handle the `ExceptionGroup` with `except*`
- [ ] CPU-bound parallelism uses processes (GIL); per-task state uses `ContextVar`, not `threading.local`
