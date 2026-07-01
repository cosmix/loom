---
name: loom-documentation
description: Creates and maintains technical documentation including API docs, READMEs, architecture docs, changelogs, ADRs, and inline code comments. Use for any documentation task from code-level docstrings to project-level guides.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
triggers:
  - documentation
  - docs
  - document
  - README
  - tutorial
  - guide
  - reference
  - changelog
  - ADR
  - architecture decision record
  - docstring
  - JSDoc
  - rustdoc
  - pydoc
  - markdown
  - mdx
  - api docs
  - user guide
  - developer guide
  - inline comments
  - code comments
---

# Documentation

## Overview

Create and maintain documentation *artifacts* and their required structure: docstrings, inline comments, API references, READMEs, ADRs, changelogs, and architecture docs. This skill owns WHAT each artifact must contain. For prose mechanics (voice, clarity, sentence craft) and the Diátaxis mode framework, see `loom-technical-writing`. For visuals, see `loom-diagramming`; for API-spec files (OpenAPI), see `loom-api-documentation`.

## Artifact → structure map

Pick the artifact by audience and Diátaxis mode, then fill its required structure. Never mix modes in one document (a reference is not a tutorial).

| Artifact                   | Diátaxis mode      | Must contain                                                     |
| -------------------------- | ------------------ | --------------------------------------------------------------- |
| README                     | how-to + reference | one-line pitch, install, quick-start, links out                 |
| Tutorial / getting-started | tutorial           | ordered steps, a path guaranteed to work, expected output       |
| How-to guide               | how-to             | goal-titled, prerequisites, numbered steps, verification        |
| API reference              | reference          | every public symbol: params, returns, errors, ≥1 example        |
| ADR                        | explanation        | context, decision, consequences, alternatives, status           |
| Changelog                  | reference          | changes grouped per version, dates, breaking changes flagged    |
| Docstring                  | reference          | purpose, params, returns, raises/panics, example                |
| Inline comment             | explanation        | WHY only — never restate the code                               |

First step on any task: identify the audience (developer / end-user / operator / contributor), then read the code so docs match real behavior — never document from assumption.

## Code documentation

**Docstrings** (rustdoc / JSDoc / pydoc / godoc): document the *contract*, not the implementation. Cover purpose, each param with type and constraints, return, errors/panics/raises, side effects, and a runnable example for any non-trivial API. Public symbols get docs; private ones only when non-obvious.

**Inline comments:** explain WHY, not WHAT. Reserve for non-obvious business rules, workarounds (link the issue/ticket), invariants, and hazards. Delete comments that restate code — they rot and mislead. `TODO`/`FIXME` must reference a tracking issue.

```rust
// BAD — restates the code; rots silently
counter += 1; // increment counter

// GOOD — encodes a non-obvious rule the code can't express
counter += 1; // skip row 0: it is always the CSV header

// GOOD — flags a business rule a future reader would violate
if user.is_valid() { // users created before 2020 have no email verification
```

### Python docstring (Google style)

```python
def calculate_shipping_cost(weight: float, destination: str, express: bool = False) -> Decimal:
    """Calculate shipping cost from package weight and destination.

    Applies tiered pricing by weight bracket; surcharges for international
    and express delivery.

    Args:
        weight: Package weight in kg. Must be positive.
        destination: ISO 3166-1 alpha-2 country code (e.g., 'US', 'GB').
        express: Express delivery (2-3 days) vs standard (5-7). Default False.

    Returns:
        Shipping cost in USD as a Decimal.

    Raises:
        ValueError: If weight is not positive.
        InvalidDestinationError: If country code is unrecognized.

    Example:
        >>> calculate_shipping_cost(2.5, 'US')
        Decimal('12.50')
    """
```

### Rust doc comment (with doctest)

Doctests in rustdoc are compiled and run by `cargo test` — keep them correct and current.

````rust
/// Manages authentication and session handling. Tokens are JWTs with a
/// 24-hour expiration.
///
/// # Examples
///
/// ```
/// let auth = AuthService::new(config);
/// let token = auth.login("user@example.com", "password").await?;
/// let user = auth.validate_token(&token)?;
/// ```
///
/// # Errors
///
/// - `AuthError::InvalidCredentials` — login failed.
/// - `AuthError::TokenExpired` — token past expiry.
pub struct AuthService { /* fields */ }
````

## README

Order by decreasing importance — most readers scan the top and leave. Required: project name + one-line description, install, quick-start (working snippet), then links out to deeper docs. Add contributing/license and status badges as needed. Keep the README a launchpad, not a manual; move depth into linked docs.

```markdown
# Project Name

One-line description of what this does and for whom.

## Install

    npm install project-name

## Quick Start

    const project = require('project-name');
    project.doSomething();

## Documentation

See [full docs](link).

## License

MIT
```

## API reference

Document every public endpoint: method + path, auth requirement, request schema, a concrete request/response example, and *all* error responses with codes. Tables for params/errors keep it scannable.

```markdown
## Create User — `POST /api/v1/users`

**Auth:** Required (Admin role)

**Request body:**

| Field | Type   | Required | Description                  |
| ----- | ------ | -------- | ---------------------------- |
| email | string | Yes      | Valid email address          |
| name  | string | Yes      | Full name (2-100 characters) |
| role  | string | No       | User role. Default: "member" |

**Success (201):** returns the created user object with `id` and `createdAt`.

**Errors:**

| Status | Code          | Description               |
| ------ | ------------- | ------------------------- |
| 400    | INVALID_EMAIL | Email format is invalid   |
| 409    | EMAIL_EXISTS  | Email already registered  |
| 403    | FORBIDDEN     | Insufficient permissions  |
```

## ADR

Record *significant, hard-to-reverse* decisions — the ones a future maintainer will ask "why on earth did they…". One decision per file, numbered, immutable once accepted (supersede with a new ADR rather than editing).

```markdown
# ADR-001: Use Event Sourcing for Audit Trail

**Status:** Accepted · **Date:** 2024-01-15

**Context:** Regulatory compliance requires a complete audit trail of financial
records. Update-in-place loses historical state.

**Decision:** Store all state changes as immutable events; derive current state
by replaying them.

**Consequences:**
- (+) Complete audit trail by design; time-travel debugging.
- (-) More storage; querying current state is more complex; team learning curve.

**Alternatives considered:**
- DB triggers + audit table — rejected: trigger maintenance burden.
- CDC — rejected: couples to database technology.
```

## Changelog

Follow [Keep a Changelog](https://keepachangelog.com/) + [SemVer](https://semver.org/). Group by type (Added / Changed / Deprecated / Removed / Fixed / Security); keep an `[Unreleased]` section at top; date each release; call out breaking changes prominently; use imperative mood consistent with commit style. Prefer generating from conventional commits.

```markdown
# Changelog

## [Unreleased]

## [1.2.0] - 2024-01-15

### Added
- User profile management (#123)

### Changed
- Improve validation error messages

### Fixed
- Race condition in payment processing (#156)
```

## Anti-patterns

- Documenting private implementation details that should stay internal.
- Docs that duplicate what the code already states (comments restating code, docstrings echoing signatures).
- Vague hedging ("might", "usually", "sometimes") instead of stating actual behavior.
- Orphaned docs with no inbound/outbound links; untested examples that no longer compile.
- Skipping error cases and edge conditions.
- Letting docs drift from code — update docs in the same PR as the code change.

## Verify before done

- [ ] Every public API / endpoint / config option is documented.
- [ ] Code examples actually compile and run (rustdoc doctests pass; snippets tested).
- [ ] Errors, edge cases, and side effects are covered — not just the happy path.
- [ ] Correct artifact + single Diátaxis mode; audience-appropriate depth.
- [ ] No orphaned pages; new docs are linked from an index/README.
- [ ] Docs match current code behavior (updated in the same change).
