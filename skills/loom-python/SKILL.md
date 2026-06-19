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

This skill provides comprehensive guidance for writing idiomatic, maintainable, and production-quality Python code across all domains: web applications, data processing, machine learning, and general-purpose scripting. It covers modern Python practices including type hints, async programming, testing patterns, proper packaging, data engineering workflows, and ML model development.

## Key Concepts

### Type Hints (typing module)

On 3.9+ use built-in generics (`list[T]`, `dict[K, V]`); on 3.10+ use `X | Y` and `X | None` instead of `Union`/`Optional`; import `Callable`/`Sequence`/`Mapping`/`Iterator` from `collections.abc`, not `typing`.

```python
from collections.abc import Callable, Sequence, Mapping, Iterator, AsyncIterator
from typing import TypeVar, Generic

T = TypeVar('T')
K = TypeVar('K')
V = TypeVar('V')

def process_items(items: Sequence[T], transform: Callable[[T], T]) -> list[T]:
    return [transform(item) for item in items]

class Repository(Generic[T]):
    def __init__(self) -> None:
        self._items: dict[str, T] = {}

    def get(self, key: str) -> T | None:
        return self._items.get(key)

    def set(self, key: str, value: T) -> None:
        self._items[key] = value
```

### Async/Await Patterns

Prefer `asyncio.TaskGroup` (3.11+) over `gather()`: on failure it cancels the remaining sibling tasks, awaits them, and re-raises all errors as an `ExceptionGroup` (handled with `except*`). `gather()` does NOT cancel siblings on failure — the docs state they "won't be cancelled and will continue to run" — and `return_exceptions=True` silently mixes return values and exception objects into one list. Reserve `gather()` for genuinely independent fire-and-forget results.

```python
import asyncio
from collections.abc import AsyncIterator

async def fetch_data(url: str) -> dict:
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.json()

async def process_batch(urls: list[str]) -> list[dict]:
    async with asyncio.TaskGroup() as tg:
        tasks = [tg.create_task(fetch_data(url)) for url in urls]
    # any failure cancels siblings; ExceptionGroup raised on exit
    return [t.result() for t in tasks]

# Handle the ExceptionGroup a TaskGroup raises:
try:
    await process_batch(urls)
except* ConnectionError as eg:
    for exc in eg.exceptions:
        logger.error("fetch failed: %s", exc)

async def stream_items(source: AsyncIterator[bytes]) -> AsyncIterator[dict]:
    async for chunk in source:
        yield json.loads(chunk)
```

### Context Managers

```python
from contextlib import contextmanager, asynccontextmanager
from typing import Iterator, AsyncIterator

@contextmanager
def managed_resource(name: str) -> Iterator[Resource]:
    resource = Resource(name)
    try:
        resource.acquire()
        yield resource
    finally:
        resource.release()

@asynccontextmanager
async def async_transaction(db: Database) -> AsyncIterator[Transaction]:
    tx = await db.begin()
    try:
        yield tx
        await tx.commit()
    except Exception:
        await tx.rollback()
        raise
```

### Decorators

```python
from functools import wraps
from typing import Callable, ParamSpec, TypeVar

P = ParamSpec('P')
R = TypeVar('R')

def retry(max_attempts: int = 3) -> Callable[[Callable[P, R]], Callable[P, R]]:
    def decorator(func: Callable[P, R]) -> Callable[P, R]:
        @wraps(func)
        def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
            last_exception: Exception | None = None
            for attempt in range(max_attempts):
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    last_exception = e
            raise last_exception
        return wrapper
    return decorator
```

### Generators

```python
from typing import Generator, Iterator

def paginate(items: Sequence[T], page_size: int) -> Generator[list[T], None, None]:
    for i in range(0, len(items), page_size):
        yield list(items[i:i + page_size])

def read_chunks(file_path: str, chunk_size: int = 8192) -> Iterator[bytes]:
    with open(file_path, 'rb') as f:
        while chunk := f.read(chunk_size):
            yield chunk
```

## Best Practices

### PEP 8 Compliance

- Use 4 spaces for indentation (never tabs)
- Maximum line length of 88 characters (black default) or 79 (strict PEP 8)
- Use snake_case for functions and variables, PascalCase for classes
- Two blank lines before top-level definitions, one blank line between methods
- Imports at the top: standard library, third-party, local (separated by blank lines)

### Modern Python Features (3.10+)

```python
# Structural pattern matching
match command:
    case {"action": "create", "name": str(name)}:
        create_resource(name)
    case {"action": "delete", "id": int(id_)}:
        delete_resource(id_)
    case _:
        raise ValueError("Unknown command")

# Union types with |
def process(value: int | str | None) -> str:
    ...

# Self type for fluent interfaces
from typing import Self

class Builder:
    def with_name(self, name: str) -> Self:
        self._name = name
        return self
```

### Packaging with pyproject.toml

```toml
[project]
name = "mypackage"
version = "0.1.0"
description = "A sample package"
requires-python = ">=3.11"
dependencies = [
    "httpx>=0.25.0",
    "pydantic>=2.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0.0",
    "pytest-asyncio>=0.21.0",
    "mypy>=1.0.0",
    "ruff>=0.1.0",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.ruff]
line-length = 88
target-version = "py311"

# Since Ruff 0.1 (formatter), lint settings MUST live under [tool.ruff.lint],
# not the top level (top-level placement is deprecated and warns).
[tool.ruff.lint]
select = ["E4", "E7", "E9", "F", "I", "UP"]  # UP (pyupgrade) enforces typing currency

[tool.ruff.format]
quote-style = "double"

[tool.mypy]
strict = true
python_version = "3.11"

[tool.pytest.ini_options]
asyncio_mode = "auto"  # pytest-asyncio defaults to "strict"; async tests are silently uncollected without this
```

## Web Framework Patterns

### FastAPI Applications

```python
from fastapi import FastAPI, Depends, HTTPException, status
from pydantic import BaseModel, Field
from typing import Annotated

app = FastAPI(title="API Service", version="1.0.0")

class UserCreate(BaseModel):
    email: str = Field(..., pattern=r'^[\w\.-]+@[\w\.-]+\.\w+$')
    name: str = Field(..., min_length=1, max_length=100)

class User(UserCreate):
    id: int

async def get_db() -> AsyncIterator[AsyncSession]:
    async with AsyncSession(engine) as session:
        yield session

@app.post("/users/", response_model=User, status_code=status.HTTP_201_CREATED)
async def create_user(
    user: UserCreate,
    db: Annotated[AsyncSession, Depends(get_db)]
) -> User:
    db_user = await UserService(db).create(user)
    return User(id=db_user.id, email=db_user.email, name=db_user.name)

@app.get("/users/{user_id}", response_model=User)
async def get_user(
    user_id: int,
    db: Annotated[AsyncSession, Depends(get_db)]
) -> User:
    user = await UserService(db).get(user_id)
    if not user:
        raise HTTPException(status_code=404, detail="User not found")
    return user
```

### Django Patterns

```python
from django.db import models, transaction
from django.core.validators import EmailValidator
from typing import Self

class TimeStampedModel(models.Model):
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        abstract = True

class User(TimeStampedModel):
    email = models.EmailField(unique=True, validators=[EmailValidator()])
    name = models.CharField(max_length=100)
    is_active = models.BooleanField(default=True)

    class Meta:
        db_table = 'users'
        indexes = [
            models.Index(fields=['email']),
            models.Index(fields=['created_at']),
        ]

    @classmethod
    def create_with_profile(cls, email: str, name: str) -> Self:
        with transaction.atomic():
            user = cls.objects.create(email=email, name=name)
            Profile.objects.create(user=user)
            return user
```

## Data Engineering Patterns

### Pandas Data Processing

```python
import pandas as pd
import numpy as np
from typing import Callable

def load_and_clean(file_path: str) -> pd.DataFrame:
    df = pd.read_csv(file_path, parse_dates=['timestamp'])

    # Handle missing values
    df['amount'] = df['amount'].fillna(0)
    df['category'] = df['category'].fillna('unknown')

    # Type conversions
    df['user_id'] = df['user_id'].astype('Int64')
    df['amount'] = df['amount'].astype('float64')

    # Remove duplicates
    df = df.drop_duplicates(subset=['user_id', 'timestamp'])

    return df

def aggregate_by_window(
    df: pd.DataFrame,
    window: str = '1D',
    agg_funcs: dict[str, str | list[str]] = None
) -> pd.DataFrame:
    if agg_funcs is None:
        agg_funcs = {'amount': ['sum', 'mean', 'count']}

    return (df
        .set_index('timestamp')
        .groupby('category')
        .resample(window)
        .agg(agg_funcs)
        .reset_index())

def apply_transformation(
    df: pd.DataFrame,
    transform: Callable[[pd.Series], pd.Series],
    columns: list[str]
) -> pd.DataFrame:
    df_copy = df.copy()
    for col in columns:
        df_copy[col] = transform(df_copy[col])
    return df_copy

# Vectorized operations for performance
def calculate_features(df: pd.DataFrame) -> pd.DataFrame:
    df['amount_log'] = np.log1p(df['amount'])
    df['amount_zscore'] = (df['amount'] - df['amount'].mean()) / df['amount'].std()
    df['is_weekend'] = df['timestamp'].dt.dayofweek.isin([5, 6])
    return df
```

### Dask for Large Datasets

```python
import dask.dataframe as dd
from dask.diagnostics import ProgressBar

def process_large_dataset(input_path: str, output_path: str) -> None:
    # Read partitioned data
    ddf = dd.read_parquet(input_path, engine='pyarrow')

    # Lazy transformations
    ddf = ddf[ddf['amount'] > 0]
    ddf['amount_usd'] = ddf['amount'] * ddf['exchange_rate']

    # Aggregation
    result = ddf.groupby('category').agg({
        'amount_usd': ['sum', 'mean', 'count'],
        'user_id': 'nunique'
    })

    # Execute and save
    with ProgressBar():
        result.compute().to_parquet(output_path)

def parallel_apply(
    ddf: dd.DataFrame,
    func: Callable[[pd.DataFrame], pd.DataFrame],
    meta: dict[str, type]
) -> dd.DataFrame:
    return ddf.map_partitions(func, meta=meta)
```

### NumPy Numerical Computing

```python
import numpy as np
from numpy.typing import NDArray

def moving_average(
    data: NDArray[np.float64],
    window_size: int
) -> NDArray[np.float64]:
    return np.convolve(data, np.ones(window_size), 'valid') / window_size

def normalize_features(
    X: NDArray[np.float64],
    axis: int = 0
) -> tuple[NDArray[np.float64], NDArray[np.float64], NDArray[np.float64]]:
    mean = np.mean(X, axis=axis, keepdims=True)
    std = np.std(X, axis=axis, keepdims=True)
    X_normalized = (X - mean) / (std + 1e-8)
    return X_normalized, mean, std

def batch_process(
    data: NDArray[np.float64],
    batch_size: int
) -> list[NDArray[np.float64]]:
    n_samples = data.shape[0]
    return [data[i:i+batch_size] for i in range(0, n_samples, batch_size)]
```

## Machine Learning Patterns

### Scikit-learn Pipelines

```python
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler
from sklearn.ensemble import RandomForestClassifier
from sklearn.model_selection import cross_val_score, GridSearchCV
from sklearn.base import BaseEstimator, TransformerMixin
import numpy as np

class CustomFeatureTransformer(BaseEstimator, TransformerMixin):
    def __init__(self, log_transform: bool = True):
        self.log_transform = log_transform

    def fit(self, X: NDArray, y: NDArray | None = None) -> Self:
        return self

    def transform(self, X: NDArray) -> NDArray:
        X_copy = X.copy()
        if self.log_transform:
            X_copy = np.log1p(np.abs(X_copy))
        return X_copy

def build_pipeline() -> Pipeline:
    return Pipeline([
        ('features', CustomFeatureTransformer(log_transform=True)),
        ('scaler', StandardScaler()),
        ('classifier', RandomForestClassifier(random_state=42))
    ])

def train_with_cv(
    X: NDArray,
    y: NDArray,
    pipeline: Pipeline,
    cv: int = 5
) -> dict[str, float]:
    scores = cross_val_score(pipeline, X, y, cv=cv, scoring='f1_macro')
    return {
        'mean_score': scores.mean(),
        'std_score': scores.std(),
        'scores': scores.tolist()
    }

def hyperparameter_search(
    X: NDArray,
    y: NDArray,
    pipeline: Pipeline
) -> tuple[Pipeline, dict]:
    param_grid = {
        'classifier__n_estimators': [100, 200, 300],
        'classifier__max_depth': [10, 20, None],
        'features__log_transform': [True, False]
    }

    search = GridSearchCV(
        pipeline,
        param_grid,
        cv=5,
        scoring='f1_macro',
        n_jobs=-1,
        verbose=1
    )

    search.fit(X, y)
    return search.best_estimator_, search.best_params_
```

### PyTorch Model Training

```python
import torch
import torch.nn as nn
from torch.utils.data import Dataset, DataLoader
from typing import Callable

class CustomDataset(Dataset[tuple[torch.Tensor, torch.Tensor]]):
    def __init__(self, X: NDArray, y: NDArray, transform: Callable | None = None):
        self.X = torch.from_numpy(X).float()
        self.y = torch.from_numpy(y).long()
        self.transform = transform

    def __len__(self) -> int:
        return len(self.X)

    def __getitem__(self, idx: int) -> tuple[torch.Tensor, torch.Tensor]:
        x, y = self.X[idx], self.y[idx]
        if self.transform:
            x = self.transform(x)
        return x, y

class MLP(nn.Module):
    def __init__(self, input_dim: int, hidden_dims: list[int], output_dim: int):
        super().__init__()
        layers = []
        prev_dim = input_dim

        for hidden_dim in hidden_dims:
            layers.extend([
                nn.Linear(prev_dim, hidden_dim),
                nn.ReLU(),
                nn.BatchNorm1d(hidden_dim),
                nn.Dropout(0.3)
            ])
            prev_dim = hidden_dim

        layers.append(nn.Linear(prev_dim, output_dim))
        self.network = nn.Sequential(*layers)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.network(x)

def train_epoch(
    model: nn.Module,
    dataloader: DataLoader,
    criterion: nn.Module,
    optimizer: torch.optim.Optimizer,
    device: torch.device
) -> float:
    model.train()
    total_loss = 0.0

    for X_batch, y_batch in dataloader:
        X_batch = X_batch.to(device)
        y_batch = y_batch.to(device)

        optimizer.zero_grad()
        outputs = model(X_batch)
        loss = criterion(outputs, y_batch)
        loss.backward()
        optimizer.step()

        total_loss += loss.item() * X_batch.size(0)

    return total_loss / len(dataloader.dataset)

@torch.no_grad()
def evaluate(
    model: nn.Module,
    dataloader: DataLoader,
    device: torch.device
) -> tuple[float, NDArray]:
    model.eval()
    all_preds = []
    all_labels = []

    for X_batch, y_batch in dataloader:
        X_batch = X_batch.to(device)
        outputs = model(X_batch)
        preds = outputs.argmax(dim=1).cpu().numpy()

        all_preds.extend(preds)
        all_labels.extend(y_batch.numpy())

    accuracy = np.mean(np.array(all_preds) == np.array(all_labels))
    return accuracy, np.array(all_preds)
```

## Common Patterns

### Dataclasses and Pydantic Models

```python
from dataclasses import dataclass, field
from pydantic import BaseModel, Field, field_validator

@dataclass
class Config:
    host: str
    port: int = 8080
    tags: list[str] = field(default_factory=list)

# By default @dataclass generates __eq__ and sets __hash__ = None (instances
# are unhashable — cannot be set members or dict keys). Use @dataclass(frozen=True)
# for a usable __hash__ on immutable value objects; only mutable dataclasses (like
# Config above) keep the default. Python 3.10+ also offers slots=True (memory/typo
# safety) and kw_only=True.

class UserCreate(BaseModel):
    email: str = Field(..., min_length=5)
    name: str = Field(..., max_length=100)

    @field_validator('email')
    @classmethod
    def validate_email(cls, v: str) -> str:
        if '@' not in v:
            raise ValueError('Invalid email')
        return v.lower()
```

### Testing with pytest

Async fixtures must use `@pytest_asyncio.fixture`, not `@pytest.fixture` — under the default strict mode a plain `@pytest.fixture` on an `async def` yields a coroutine, not the awaited value. Session/module-scoped async fixtures need `loop_scope=`; the `event_loop` fixture was removed in pytest-asyncio 1.0.

```python
import pytest
import pytest_asyncio
from unittest.mock import AsyncMock, patch

@pytest.fixture
def client() -> TestClient:
    return TestClient(app)

@pytest_asyncio.fixture
async def db_session() -> AsyncIterator[AsyncSession]:
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    async with AsyncSession(engine) as session:
        yield session

class TestUserService:
    @pytest.mark.asyncio
    async def test_create_user(self, db_session: AsyncSession) -> None:
        service = UserService(db_session)
        user = await service.create(name="test", email="test@example.com")
        assert user.id is not None

    @pytest.mark.parametrize("email,valid", [
        ("user@example.com", True),
        ("invalid", False),
        ("", False),
    ])
    def test_email_validation(self, email: str, valid: bool) -> None:
        if valid:
            User(email=email, name="test")
        else:
            with pytest.raises(ValueError):
                User(email=email, name="test")

    @patch("mymodule.external_api")
    async def test_with_mock(self, mock_api: AsyncMock) -> None:
        mock_api.fetch.return_value = {"status": "ok"}
        result = await process_with_api()
        mock_api.fetch.assert_called_once()
```

## Anti-Patterns

### Avoid These Practices

```python
# BAD: Mutable default arguments
def append_to(item, target=[]):  # Bug: shared list across calls
    target.append(item)
    return target

# GOOD: Use None and create new list
def append_to(item, target=None):
    if target is None:
        target = []
    target.append(item)
    return target

# BAD: Bare except clauses
try:
    risky_operation()
except:  # Catches SystemExit, KeyboardInterrupt too
    pass

# GOOD: Catch specific exceptions
try:
    risky_operation()
except (ValueError, RuntimeError) as e:
    logger.error(f"Operation failed: {e}")

# BAD: String formatting with + for complex strings
message = "User " + name + " has " + str(count) + " items"

# GOOD: f-strings
message = f"User {name} has {count} items"

# BAD: Checking type with type()
if type(obj) == list:
    ...

# GOOD: Use isinstance for type checking
if isinstance(obj, list):
    ...

# BAD: Not using context managers for resources
f = open("file.txt")
data = f.read()
f.close()

# GOOD: Always use context managers
with open("file.txt") as f:
    data = f.read()

# BAD: Global mutable state
_cache = {}

def get_cached(key):
    return _cache.get(key)

# GOOD: Encapsulate state in classes or use dependency injection
class Cache:
    def __init__(self):
        self._store: dict[str, Any] = {}

    def get(self, key: str) -> Any | None:
        return self._store.get(key)

# BAD: Late-binding closures in a loop
fns = [lambda: i for i in range(3)]
[f() for f in fns]  # [2, 2, 2] - all closures share the same 'i' cell,
                    # resolved at call time after the loop has finished

# GOOD: Bind the value eagerly with a default argument (or a factory function)
fns = [lambda i=i: i for i in range(3)]
[f() for f in fns]  # [0, 1, 2]
```

### Quick Pattern Swaps

```python
# BAD: Equality checks against None
if result == None:
    handle_missing_result()

# GOOD: Use identity checks for singletons
if result is None:
    handle_missing_result()

# BAD: A broad try block that hides the failing line
try:
    value = payload["count"]
    return normalize(value)
except KeyError:
    return 0

# GOOD: Keep the try block as small as possible
try:
    value = payload["count"]
except KeyError:
    return 0
return normalize(value)

# BAD: Treating "empty" and "missing" as the same thing
if items:
    process(items)

# GOOD: Check for None explicitly when empties are valid input
if items is not None:
    process(items)
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance for production Python. Each item states the mechanism, not just the rule.

### Typing Idioms

**Use PEP 695 type-parameter syntax for generics and aliases (3.12+).** Square brackets on a class/def declare type parameters in their own scope — no module-level `TypeVar` or `Generic[T]` base needed — and the `type` statement defines lazily-evaluated aliases so forward/recursive references work without quoting. Variance is inferred. Keep `TypeVar`/`Generic`/`ParamSpec` only for libraries supporting < 3.12; do not mix old `TypeVar` and new syntax in one class.

```python
class Stack[T]:
    def __init__(self) -> None:
        self._items: list[T] = []

def first[T](items: list[T]) -> T:
    return items[0]

type Vector = list[float]
def max_item[T: (int, float)](items: list[T]) -> T: ...  # inline constraints
```

**Annotate arguments with abstract types, return values with concrete types.** Accept `Iterable`/`Sequence`/`Mapping` from `collections.abc` to maximize caller flexibility; return `list`/`dict` so callers know exactly what they get.

```python
from collections.abc import Iterable

def deduplicate(items: Iterable[str]) -> list[str]:
    return list(dict.fromkeys(items))
```

**Prefer `TypeIs` over `TypeGuard` for narrowing predicates (3.13+, PEP 742).** `TypeGuard` narrows only the `True` branch and permits unsound narrowing to an unrelated type. `TypeIs` narrows in BOTH branches and requires the narrowed type to be compatible with the input — sound for the common `isinstance`-style predicate. Both are in `typing_extensions` for older Pythons.

```python
from typing import TypeIs  # or typing_extensions

def is_str_list(val: list[object]) -> TypeIs[list[str]]:
    return all(isinstance(x, str) for x in val)
# narrows to list[str] in the if-branch AND back to list[object] in else
```

**Use `ParamSpec` + `Concatenate` to keep decorators signature-transparent (PEP 612).** Without `ParamSpec`, a wrapper typed `(*args: Any, **kwargs: Any) -> Any` erases all parameter info for callers and IDEs. `P.args`/`P.kwargs` capture the full parameter list as a unit so the original signature flows through unchanged; pair with a `TypeVar` for the return. `Concatenate[X, P]` models injecting/removing a leading parameter (e.g. supplying a `Session` first). The skill's `retry` decorator above already follows this.

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

**Prefer `asyncio.timeout()` (3.11+) over `wait_for()`.** It participates in cooperative cancellation; `wait_for(gather(...))` cancels the gather but the gathered coroutines keep running.

**Use `contextvars.ContextVar`, not `threading.local()`, for per-task state in async code.** asyncio runs all coroutines on one thread, so every coroutine shares the same `threading.local()` namespace — isolation is silently defeated. Each `asyncio.Task` runs in a shallow copy of the current `Context`, so a `ContextVar` set in one task does not leak to siblings (and, by design, a child's mutation does not propagate back to the parent).

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

**Use `ExitStack` for dynamic/conditional cleanup.** It handles a variable number of resources, conditional cleanup, and rollback during `__enter__` — cases fixed `with`-nesting cannot. Expert API points: `pop_all()` transfers callbacks to a fresh stack (invoking nothing) for "commit on success" ownership transfer; `push()` registers a context manager's `__exit__` and CAN suppress exceptions; `callback()` registers a plain function that CANNOT suppress (never passed exception details). Reusable but not reentrant; `AsyncExitStack` exposes `aclose()`.

```python
from contextlib import ExitStack

def open_all(filenames: list[str]):
    with ExitStack() as stack:
        files = [stack.enter_context(open(f)) for f in filenames]
        keep_open = stack.pop_all()   # transfer ownership; stack now empty
    return files, keep_open           # caller closes via keep_open.close()
```

### Data Model Gotchas

**Return `NotImplemented` (never raise `NotImplementedError`) from arithmetic/comparison dunders.** When an operand type can't be handled, returning the `NotImplemented` singleton lets the interpreter try the reflected operation (`other.__radd__`) or fall back to identity comparison; raising `NotImplementedError` short-circuits that chain and breaks interop. As of Python 3.14, evaluating `NotImplemented` in a boolean context raises `TypeError` (previously `True` + `DeprecationWarning` since 3.9), so returning it from a predicate by mistake now fails loudly.

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

**Data descriptors shadow the instance `__dict__`; non-data descriptors do not.** Lookup order in `object.__getattribute__` is: data descriptors (define `__set__`/`__delete__`) > instance `__dict__` > non-data descriptors (`__get__` only) and other class vars > `__getattr__`. This is why plain methods can be shadowed by instance attributes, while `property` and `__slots__` (data descriptors) always win. Overriding `__getattribute__` entirely disables the descriptor protocol. Use `__set_name__` so a descriptor learns its own attribute name.

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

### Dataclass Gotchas

Beyond the hashability rule noted under Common Patterns:

- **Field ordering is enforced across inheritance:** a field without a default cannot follow one with a default, even across base/subclass boundaries (`TypeError` at class definition).
- **`replace()` does not copy `init=False` fields:** it re-runs `__post_init__`, recomputing them from new values; passing an `init=False` field in the changes is a `ValueError`.
- **Python 3.13 changed generated `__eq__`** from tuple comparison to field-by-field. This flips NaN behavior: `C(float("nan")) == C(float("nan"))` was `True` (tuple identity short-circuit) but is `False` in 3.13+.
- **`slots=True` returns a NEW class object;** always enumerate fields with `fields()`, not `__slots__`.
- **Frozen `__post_init__` must use `object.__setattr__`** to set computed/`init=False` fields — `self.x = ...` raises `FrozenInstanceError`. And `frozen` is not transitive: a frozen subclass of a non-frozen base does not freeze the base's fields.

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

**`multiprocessing` `fork()` with live threads deadlocks; use `spawn` or `forkserver`.** POSIX `fork()` copies only the calling thread but duplicates all lock state, so the child can inherit locks (logging, NumPy BLAS, connection pools) that are held forever with no thread to release them. Python 3.12 warns on `os.fork()` in a multi-threaded process; 3.14 changed the POSIX default from `fork` to `forkserver`. Set the method explicitly.

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

**`typing.TypeAlias` and `typing.AnyStr` are deprecated.** `TypeAlias` is deprecated since 3.12 in favor of the `type` statement (lazily-evaluated, native forward references). `AnyStr` is deprecated since 3.13 (removed from `typing.__all__` with a `DeprecationWarning` in 3.16, removed entirely in 3.18) — replace with a PEP 695 constrained type parameter `def f[T: (str, bytes)]` on 3.12+, or `TypeVar("S", str, bytes)` on older Pythons.

**Avoid `from __future__ import annotations` in runtime-introspective code.** PEP 563 stringifies all annotations, breaking libraries that read `__annotations__` or call `typing.get_type_hints()` at runtime: re-evaluating the strings fails when referenced types live under `TYPE_CHECKING` guards or local scopes (historic source of dataclasses `ClassVar`/`InitVar` bugs). PEP 563 was superseded by PEP 649 (deferred annotations) in 3.14; PEP 749 schedules the future-import for eventual deprecation. Quote only the forward references that genuinely need it.

```python
def process(value: int | str | None) -> None: ...

class Node:
    def children(self) -> list["Node"]: ...  # quote only the real forward ref
```
