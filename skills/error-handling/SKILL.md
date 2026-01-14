---
name: error-handling
description: Comprehensive error handling patterns and strategies including Rust Result/Option, API error responses, data pipeline error handling, and security-aware error handling. Use when implementing exception handling, error recovery, retry logic, circuit breakers, fallback mechanisms, graceful degradation, or designing error hierarchies. Triggers: error, exception, try, catch, throw, raise, Result, Option, panic, recover, retry, fallback, graceful degradation, circuit breaker, error boundary, 500, 4xx, 5xx, thiserror, anyhow, RFC 7807, error propagation, error messages, stack trace.
---

# Error Handling

## Overview

Error handling is a critical aspect of robust software development. This skill covers error types and hierarchies, recovery strategies, propagation patterns, user-friendly messaging, contextual logging, and language-specific implementations (Rust, Python, TypeScript).

## Agent Delegation

- **senior-software-engineer** (Opus) - Error architecture design, choosing error strategies
- **software-engineer** (Sonnet) - Implements error handling patterns
- **security-engineer** (Opus) - Secure error handling (no info leakage, sanitization)
- **senior-infrastructure-engineer** (Opus) - Infrastructure error handling (retry, circuit breakers)

## Instructions

### 1. Design Error Hierarchies

Create structured error types that provide clear categorization.

#### Rust: thiserror and anyhow

```rust
// Using thiserror for library errors
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("validation failed: {0}")]
    Validation(String),

    #[error("resource not found: {resource_type} with id {id}")]
    NotFound {
        resource_type: String,
        id: String,
    },

    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("external service error: {service}")]
    ExternalService {
        service: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

// Using anyhow for application errors
use anyhow::{Context, Result};

fn process_order(order_id: &str) -> Result<Order> {
    let order = fetch_order(order_id)
        .context("Failed to fetch order from database")?;

    validate_order(&order)
        .context(format!("Order {} validation failed", order_id))?;

    Ok(order)
}
```

#### Python

```python
# Python example
class AppError(Exception):
    """Base application error"""
    def __init__(self, message: str, code: str, details: dict = None):
        self.message = message
        self.code = code
        self.details = details or {}
        super().__init__(message)

class ValidationError(AppError):
    """Input validation errors"""
    pass

class NotFoundError(AppError):
    """Resource not found errors"""
    pass

class ServiceError(AppError):
    """External service errors"""
    pass
```

#### TypeScript

```typescript
class AppError extends Error {
  constructor(
    message: string,
    public code: string,
    public details?: Record<string, unknown>
  ) {
    super(message);
    this.name = this.constructor.name;
  }
}

class ValidationError extends AppError {}
class NotFoundError extends AppError {}
class ServiceError extends AppError {}
```

### 2. Implement Recovery Strategies

#### Retry with Exponential Backoff

##### Rust

```rust
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

pub async fn retry_with_backoff<T, E, F, Fut>(
    mut operation: F,
    max_retries: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempts = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempts >= max_retries => return Err(e),
            Err(e) => {
                let delay = std::cmp::min(
                    base_delay_ms * 2_u64.pow(attempts),
                    max_delay_ms
                );
                let jitter = rand::thread_rng().gen_range(0..delay / 10);
                let total_delay = delay + jitter;

                tracing::warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempts + 1,
                    max_retries,
                    e,
                    total_delay
                );

                sleep(Duration::from_millis(total_delay)).await;
                attempts += 1;
            }
        }
    }
}

// Usage
let result = retry_with_backoff(
    || async { fetch_from_api().await },
    3,
    1000,
    30000,
).await?;
```

##### Python

```python
import asyncio
from typing import TypeVar, Callable
import random

T = TypeVar('T')

async def retry_with_backoff(
    operation: Callable[[], T],
    max_retries: int = 3,
    base_delay: float = 1.0,
    max_delay: float = 60.0,
    retryable_exceptions: tuple = (ServiceError,)
) -> T:
    """Retry operation with exponential backoff and jitter."""
    for attempt in range(max_retries + 1):
        try:
            return await operation()
        except retryable_exceptions as e:
            if attempt == max_retries:
                raise
            delay = min(base_delay * (2 ** attempt), max_delay)
            jitter = random.uniform(0, delay * 0.1)
            await asyncio.sleep(delay + jitter)
```

#### Circuit Breaker

```python
import time
from enum import Enum
from dataclasses import dataclass

class CircuitState(Enum):
    CLOSED = "closed"
    OPEN = "open"
    HALF_OPEN = "half_open"

@dataclass
class CircuitBreaker:
    failure_threshold: int = 5
    recovery_timeout: float = 30.0
    half_open_max_calls: int = 3

    def __post_init__(self):
        self.state = CircuitState.CLOSED
        self.failure_count = 0
        self.last_failure_time = 0
        self.half_open_calls = 0

    def call(self, operation):
        if self.state == CircuitState.OPEN:
            if time.time() - self.last_failure_time > self.recovery_timeout:
                self.state = CircuitState.HALF_OPEN
                self.half_open_calls = 0
            else:
                raise CircuitOpenError("Circuit is open")

        try:
            result = operation()
            self._on_success()
            return result
        except Exception as e:
            self._on_failure()
            raise

    def _on_success(self):
        if self.state == CircuitState.HALF_OPEN:
            self.half_open_calls += 1
            if self.half_open_calls >= self.half_open_max_calls:
                self.state = CircuitState.CLOSED
        self.failure_count = 0

    def _on_failure(self):
        self.failure_count += 1
        self.last_failure_time = time.time()
        if self.failure_count >= self.failure_threshold:
            self.state = CircuitState.OPEN
```

#### Fallback Pattern

```typescript
async function withFallback<T>(
  primary: () => Promise<T>,
  fallback: () => Promise<T>,
  shouldFallback: (error: Error) => boolean = () => true
): Promise<T> {
  try {
    return await primary();
  } catch (error) {
    if (shouldFallback(error as Error)) {
      return await fallback();
    }
    throw error;
  }
}

// Usage
const data = await withFallback(
  () => fetchFromPrimaryAPI(),
  () => fetchFromCache(),
  (error) => error instanceof ServiceError
);
```

### 3. Error Propagation Patterns

#### Wrap and Enrich Errors

```python
def process_order(order_id: str) -> Order:
    try:
        order = fetch_order(order_id)
        validate_order(order)
        return process(order)
    except DatabaseError as e:
        raise ServiceError(
            message="Failed to process order",
            code="ORDER_PROCESSING_FAILED",
            details={"order_id": order_id, "original_error": str(e)}
        ) from e
```

#### Result Types (Rust-style)

```python
from dataclasses import dataclass
from typing import Generic, TypeVar, Union

T = TypeVar('T')
E = TypeVar('E')

@dataclass
class Ok(Generic[T]):
    value: T

@dataclass
class Err(Generic[E]):
    error: E

Result = Union[Ok[T], Err[E]]

def divide(a: float, b: float) -> Result[float, str]:
    if b == 0:
        return Err("Division by zero")
    return Ok(a / b)

# Usage
result = divide(10, 0)
match result:
    case Ok(value):
        print(f"Result: {value}")
    case Err(error):
        print(f"Error: {error}")
```

### 4. User-Friendly Error Messages

```python
ERROR_MESSAGES = {
    "VALIDATION_FAILED": "Please check your input and try again.",
    "NOT_FOUND": "The requested item could not be found.",
    "SERVICE_UNAVAILABLE": "Service is temporarily unavailable. Please try again later.",
    "UNAUTHORIZED": "Please log in to continue.",
    "FORBIDDEN": "You don't have permission to perform this action.",
}

def get_user_message(error: AppError) -> str:
    """Convert internal error to user-friendly message."""
    return ERROR_MESSAGES.get(error.code, "An unexpected error occurred. Please try again.")

def format_error_response(error: AppError, include_details: bool = False) -> dict:
    """Format error for API response."""
    response = {
        "error": {
            "code": error.code,
            "message": get_user_message(error)
        }
    }
    if include_details and error.details:
        response["error"]["details"] = error.details
    return response
```

### 5. Logging Errors with Context

```python
import logging
import traceback
from contextvars import ContextVar

request_id: ContextVar[str] = ContextVar('request_id', default='unknown')

def log_error(error: Exception, context: dict = None):
    """Log error with full context."""
    logger = logging.getLogger(__name__)

    error_context = {
        "request_id": request_id.get(),
        "error_type": type(error).__name__,
        "error_message": str(error),
        "stack_trace": traceback.format_exc(),
        **(context or {})
    }

    if isinstance(error, AppError):
        error_context["error_code"] = error.code
        error_context["error_details"] = error.details

    logger.error(
        f"Error occurred: {error}",
        extra={"structured_data": error_context}
    )
```

## Best Practices

1. **Fail Fast**: Validate inputs early and throw errors immediately rather than continuing with invalid data.

2. **Be Specific**: Create specific error types rather than using generic exceptions. This enables better handling and debugging.

3. **Preserve Context**: When wrapping errors, always preserve the original error chain using mechanisms like `from e` in Python or `cause` in other languages.

4. **Don't Swallow Errors**: Avoid empty catch blocks. At minimum, log the error.

5. **Distinguish Recoverable vs Unrecoverable**: Design your error hierarchy to clearly indicate which errors can be retried.

6. **Use Appropriate Recovery Strategies**:

   - Retry: For transient failures (network timeouts, rate limits)
   - Fallback: When alternatives exist (cache, default values)
   - Circuit Breaker: To prevent cascade failures

7. **Sanitize User-Facing Messages**: Never expose internal error details, stack traces, or sensitive information to users.

8. **Log at Boundaries**: Log errors when they cross system boundaries (API endpoints, service calls).

## Examples

### Complete Error Handling in an API Endpoint

```python
from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse

app = FastAPI()

@app.exception_handler(AppError)
async def app_error_handler(request: Request, error: AppError):
    log_error(error, {"path": request.url.path, "method": request.method})

    status_codes = {
        ValidationError: 400,
        NotFoundError: 404,
        ServiceError: 503,
    }

    status_code = status_codes.get(type(error), 500)
    return JSONResponse(
        status_code=status_code,
        content=format_error_response(error)
    )

@app.get("/orders/{order_id}")
async def get_order(order_id: str):
    circuit_breaker = get_circuit_breaker("order_service")

    async def fetch():
        return await order_service.get(order_id)

    try:
        return await retry_with_backoff(
            lambda: circuit_breaker.call(fetch),
            max_retries=3,
            retryable_exceptions=(ServiceError,)
        )
    except CircuitOpenError:
        # Fallback to cache
        cached = await cache.get(f"order:{order_id}")
        if cached:
            return cached
        raise ServiceError(
            message="Order service unavailable",
            code="SERVICE_UNAVAILABLE"
        )
```

### Error Boundary in React

```typescript
import React, { Component, ErrorInfo, ReactNode } from "react";

interface Props {
  children: ReactNode;
  fallback: ReactNode;
}

interface State {
  hasError: boolean;
  error?: Error;
}

class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Error boundary caught:", error, errorInfo);
    // Send to error tracking service
    errorTracker.captureException(error, { extra: errorInfo });
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback;
    }
    return this.props.children;
  }
}
```

## API Error Responses (RFC 7807)

RFC 7807 defines a standard format for HTTP API problem details.

### Rust Implementation

```rust
use serde::{Deserialize, Serialize};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl ProblemDetails {
    pub fn new(status: StatusCode, title: impl Into<String>) -> Self {
        Self {
            type_uri: format!("about:blank"),
            title: title.into(),
            status: status.as_u16(),
            detail: None,
            instance: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            AppError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                "Validation Failed",
                Some(msg),
            ),
            AppError::NotFound { resource_type, id } => (
                StatusCode::NOT_FOUND,
                "Resource Not Found",
                Some(format!("{} with id {} not found", resource_type, id)),
            ),
            AppError::Database(_) | AppError::Io(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                None, // Never expose internal errors
            ),
            AppError::ExternalService { service, .. } => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Service Unavailable",
                Some(format!("{} is temporarily unavailable", service)),
            ),
        };

        let mut problem = ProblemDetails::new(status, title);
        if let Some(d) = detail {
            problem = problem.with_detail(d);
        }

        (status, Json(problem)).into_response()
    }
}
```

### Python (FastAPI)

```python
from pydantic import BaseModel
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

class ProblemDetails(BaseModel):
    type: str = "about:blank"
    title: str
    status: int
    detail: str | None = None
    instance: str | None = None

@app.exception_handler(AppError)
async def app_error_handler(request: Request, error: AppError):
    status_map = {
        ValidationError: 400,
        NotFoundError: 404,
        ServiceError: 503,
    }

    status = status_map.get(type(error), 500)

    problem = ProblemDetails(
        title=error.__class__.__name__,
        status=status,
        detail=str(error) if status < 500 else None,  # Hide internals
        instance=str(request.url.path),
    )

    return JSONResponse(
        status_code=status,
        content=problem.model_dump(exclude_none=True),
        headers={"Content-Type": "application/problem+json"},
    )
```

## Data Pipeline Error Handling

Data pipelines require special error handling for partial failures and data quality issues.

### Rust Pipeline with Error Collection

```rust
use std::collections::HashMap;

#[derive(Debug)]
pub struct ProcessingResult<T> {
    pub successful: Vec<T>,
    pub failed: Vec<FailedItem>,
}

#[derive(Debug)]
pub struct FailedItem {
    pub index: usize,
    pub error: String,
    pub record: serde_json::Value,
}

pub async fn process_batch<T, F, Fut>(
    items: Vec<serde_json::Value>,
    processor: F,
) -> ProcessingResult<T>
where
    F: Fn(serde_json::Value) -> Fut,
    Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
{
    let mut successful = Vec::new();
    let mut failed = Vec::new();

    for (index, item) in items.into_iter().enumerate() {
        match processor(item.clone()).await {
            Ok(result) => successful.push(result),
            Err(e) => {
                tracing::error!("Failed to process item {}: {}", index, e);
                failed.push(FailedItem {
                    index,
                    error: e.to_string(),
                    record: item,
                });
            }
        }
    }

    ProcessingResult { successful, failed }
}

// Usage with dead letter queue
let result = process_batch(records, |record| async {
    validate_and_transform(record).await
}).await;

if !result.failed.is_empty() {
    dead_letter_queue.send(result.failed).await?;
}
```

### Python ETL Error Handling

```python
from dataclasses import dataclass
from typing import TypeVar, Callable, Generic
import logging

T = TypeVar('T')

@dataclass
class ProcessingResult(Generic[T]):
    successful: list[T]
    failed: list[dict]
    error_summary: dict[str, int]

def process_with_error_tracking(
    items: list[dict],
    processor: Callable[[dict], T],
    continue_on_error: bool = True,
) -> ProcessingResult[T]:
    successful = []
    failed = []
    error_counts = {}

    for index, item in enumerate(items):
        try:
            result = processor(item)
            successful.append(result)
        except Exception as e:
            error_type = type(e).__name__
            error_counts[error_type] = error_counts.get(error_type, 0) + 1

            logging.error(f"Failed to process item {index}: {e}")
            failed.append({
                "index": index,
                "item": item,
                "error": str(e),
                "error_type": error_type,
            })

            if not continue_on_error:
                raise

    return ProcessingResult(
        successful=successful,
        failed=failed,
        error_summary=error_counts,
    )
```

## Security-Aware Error Handling

Prevent information leakage through error messages and stack traces.

### Production vs Development Error Details

```rust
use std::env;

pub struct ErrorResponse {
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl From<AppError> for ErrorResponse {
    fn from(error: AppError) -> Self {
        let is_production = env::var("ENVIRONMENT")
            .unwrap_or_default()
            .to_lowercase() == "production";

        let message = match &error {
            AppError::Validation(msg) => msg.clone(),
            AppError::NotFound { .. } => "Resource not found".to_string(),
            _ => "An error occurred".to_string(),
        };

        let details = if is_production {
            None // Never expose stack traces or internal details
        } else {
            Some(serde_json::json!({
                "error_type": format!("{:?}", error),
                "backtrace": std::backtrace::Backtrace::capture().to_string(),
            }))
        };

        ErrorResponse { message, details }
    }
}
```

### Sanitize Errors Before Logging

```python
import re
import os

SENSITIVE_PATTERNS = [
    r'\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b',  # Email
    r'\b\d{3}-\d{2}-\d{4}\b',  # SSN
    r'\b(?:\d{4}[-\s]?){3}\d{4}\b',  # Credit card
    r'password["\']?\s*[:=]\s*["\']?[\w!@#$%^&*]+',  # Passwords
]

def sanitize_error_message(message: str) -> str:
    """Remove sensitive data from error messages before logging."""
    sanitized = message
    for pattern in SENSITIVE_PATTERNS:
        sanitized = re.sub(pattern, '[REDACTED]', sanitized, flags=re.IGNORECASE)
    return sanitized

def log_error_safely(error: Exception, context: dict = None):
    """Log errors with sanitized messages."""
    sanitized_message = sanitize_error_message(str(error))

    logger.error(
        sanitized_message,
        extra={
            "error_type": type(error).__name__,
            "context": context or {},
            "include_stacktrace": os.getenv("ENVIRONMENT") != "production",
        }
    )
```

### Rate Limiting Error Responses

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct RateLimitedErrorLogger {
    last_logged: Mutex<HashMap<String, Instant>>,
    min_interval: Duration,
}

impl RateLimitedErrorLogger {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            last_logged: Mutex::new(HashMap::new()),
            min_interval,
        }
    }

    pub async fn log_if_allowed(&self, error_key: &str, error: &dyn std::error::Error) {
        let mut last_logged = self.last_logged.lock().await;
        let now = Instant::now();

        if let Some(last_time) = last_logged.get(error_key) {
            if now.duration_since(*last_time) < self.min_interval {
                return; // Skip logging to prevent log flooding
            }
        }

        tracing::error!("Error occurred: {}", error);
        last_logged.insert(error_key.to_string(), now);
    }
}
```
