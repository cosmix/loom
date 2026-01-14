---
name: python
description: Python language expertise for writing idiomatic, production-quality Python code. Covers web frameworks (FastAPI, Django, Flask), data processing (pandas, numpy, dask), ML patterns (sklearn, pytorch), async programming, type hints, testing with pytest, packaging (pip, uv, poetry), linting (ruff, mypy, black), and PEP 8 standards. Use for any Python development including data engineering and machine learning workflows. Triggers: python, py, pip, uv, poetry, virtualenv, pytest, pydantic, fastapi, django, flask, pandas, numpy, dataclass, type hints, asyncio, mypy, ruff, black, sklearn, pytorch, tensorflow, jupyter, pipenv, conda.
---

# Python Language Expertise

## Overview

This skill provides comprehensive guidance for writing idiomatic, maintainable, and production-quality Python code across all domains: web applications, data processing, machine learning, and general-purpose scripting. It covers modern Python practices including type hints, async programming, testing patterns, proper packaging, data engineering workflows, and ML model development.

## Key Concepts

### Type Hints (typing module)

```python
from typing import Optional, Union, List, Dict, Callable, TypeVar, Generic
from collections.abc import Sequence, Mapping, Iterator

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

```python
import asyncio
from typing import AsyncIterator

async def fetch_data(url: str) -> dict:
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.json()

async def process_batch(urls: list[str]) -> list[dict]:
    tasks = [fetch_data(url) for url in urls]
    return await asyncio.gather(*tasks, return_exceptions=True)

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

[tool.mypy]
strict = true
python_version = "3.11"
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

```python
import pytest
from unittest.mock import AsyncMock, patch

@pytest.fixture
def client() -> TestClient:
    return TestClient(app)

@pytest.fixture
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
```
