---
name: documentation
description: Creates and maintains technical documentation including API docs, README files, architecture docs, changelogs, ADRs, and inline code comments. Handles all documentation needs from code-level docstrings to project-level guides. Trigger keywords: documentation, docs, document, README, tutorial, guide, reference, changelog, ADR, architecture decision record, docstring, JSDoc, rustdoc, pydoc, markdown, mdx, api docs, user guide, developer guide, inline comments, code comments.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Documentation

## Overview

This skill creates clear, comprehensive, and maintainable documentation across all levels: code documentation (docstrings, inline comments), API references, architectural documentation, README files, ADRs, changelogs, and user guides. Absorbs technical writing expertise for clarity and accessibility.

## Instructions

### 1. Assess Documentation Needs

- Identify target audience (developers, users, operators, contributors)
- Determine documentation types needed
- Review existing documentation for gaps and inconsistencies
- Understand the codebase structure and patterns

### 2. Code Documentation

**Docstrings/Doc Comments:**
- Add language-appropriate documentation (JSDoc, rustdoc, pydoc, etc.)
- Document purpose, parameters, return values, and exceptions
- Include type information when language supports it
- Provide usage examples for complex functions
- Explain non-obvious behavior and edge cases

**Inline Comments:**
- Explain WHY, not WHAT (code shows what)
- Document complex algorithms or business logic
- Flag known limitations or TODOs with issue references
- Keep comments up-to-date with code changes

### 3. API Documentation

- Document all public endpoints/methods
- Include request/response formats with examples
- Provide authentication and authorization details
- Show error responses with status codes
- Document rate limits and pagination
- Include versioning information

### 4. README Files

**Essential Sections:**
- Project name and one-line description
- Installation/setup instructions
- Quick start example
- Core features overview
- Development setup
- Testing instructions
- Contributing guidelines
- License information

**README Best Practices:**
- Put most important information first
- Use clear headings for scannability
- Include badges for build status, version, license
- Provide working code examples
- Link to detailed docs when needed

### 5. Architecture Decision Records (ADRs)

Document significant architectural decisions with:
- Context: What problem are we solving?
- Decision: What did we choose?
- Consequences: Trade-offs and implications
- Alternatives considered: What we rejected and why
- Status: Proposed, accepted, deprecated, superseded

### 6. Changelogs

Follow semantic versioning and conventional commits:
- Group changes by type (Added, Changed, Fixed, Removed, Security)
- Link to relevant issues/PRs
- Include breaking changes prominently
- Date each release
- Use present tense ("Add feature" not "Added feature")

### 7. Technical Writing Principles

**Clarity:**
- Use simple, direct language
- Define technical terms on first use
- Write short sentences (aim for 15-20 words)
- One idea per paragraph

**Structure:**
- Use consistent heading hierarchy
- Break up long sections with subheadings
- Use lists for sequential steps or related items
- Add tables for structured comparisons

**Accessibility:**
- Avoid jargon unless necessary
- Provide examples for abstract concepts
- Use active voice ("Run the command" not "The command should be run")
- Include visual aids (diagrams, code blocks) where helpful

## Best Practices

1. **Write for Your Audience**: Match complexity to reader expertise
2. **Keep It Current**: Update docs when code changes
3. **Show, Don't Just Tell**: Include working examples
4. **Be Concise**: Remove unnecessary words
5. **Structure Consistently**: Use templates and patterns
6. **Explain the Why**: Document decisions, not just facts
7. **Make It Searchable**: Use clear headings and keywords
8. **Test Your Examples**: Ensure code examples actually work
9. **Version Your Docs**: Match documentation to code versions
10. **Link Liberally**: Connect related documentation

## Templates

### README Template

```markdown
# Project Name

One-line description of what this project does.

## Features

- Key feature 1
- Key feature 2
- Key feature 3

## Installation

    npm install project-name

## Quick Start

    const project = require('project-name');
    project.doSomething();

## Documentation

See [full documentation](link) for detailed usage.

## Development

    npm install
    npm test
    npm run build

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT
```

### ADR Template

```markdown
# ADR-001: Use Event Sourcing for Audit Trail

**Status:** Accepted

**Date:** 2024-01-15

**Context:**
We need to maintain a complete audit trail of all changes to financial records
for regulatory compliance. The current update-in-place approach loses historical
state.

**Decision:**
Implement event sourcing pattern where all state changes are stored as immutable
events. Current state is derived by replaying events.

**Consequences:**

Positive:
- Complete audit trail by design
- Time-travel debugging capabilities
- Natural fit for regulatory reporting

Negative:
- Increased storage requirements
- Query complexity for current state
- Team learning curve

**Alternatives Considered:**

1. Database triggers + audit table: Rejected due to trigger maintenance burden
2. Change data capture: Rejected due to coupling to database technology
3. Manual audit logging: Rejected due to error-prone implementation
```

### Changelog Template

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- New feature description

### Changed
- Changes in existing functionality

### Fixed
- Bug fix description

## [1.2.0] - 2024-01-15

### Added
- User profile management (#123)
- Export to CSV functionality (#145)

### Changed
- Updated dependencies to latest versions
- Improved error messages for validation failures

### Fixed
- Fixed race condition in payment processing (#156)
- Corrected timezone handling in reports (#162)
```

## Examples

### Example 1: Python Docstring (Google Style)

```python
def calculate_shipping_cost(
    weight: float,
    destination: str,
    express: bool = False
) -> Decimal:
    """Calculate shipping cost based on package weight and destination.

    Applies tiered pricing based on weight brackets and adds surcharges
    for international destinations and express delivery.

    Args:
        weight: Package weight in kilograms. Must be positive.
        destination: ISO 3166-1 alpha-2 country code (e.g., 'US', 'GB').
        express: If True, uses express delivery (2-3 days).
                 Default is standard delivery (5-7 days).

    Returns:
        The calculated shipping cost in USD as a Decimal.

    Raises:
        ValueError: If weight is not positive.
        InvalidDestinationError: If country code is not recognized.

    Example:
        >>> calculate_shipping_cost(2.5, 'US')
        Decimal('12.50')
        >>> calculate_shipping_cost(2.5, 'GB', express=True)
        Decimal('45.00')
    """
```

### Example 2: Rust Documentation

```rust
/// Manages user authentication and session handling.
///
/// This service handles the complete authentication lifecycle including
/// login, token validation, and session management. Tokens are JWTs
/// with a 24-hour expiration.
///
/// # Examples
///
/// ```
/// use auth::AuthService;
///
/// let auth = AuthService::new(config);
/// let token = auth.login("user@example.com", "password").await?;
/// let user = auth.validate_token(&token)?;
/// ```
///
/// # Errors
///
/// Returns `AuthError::InvalidCredentials` if login fails.
/// Returns `AuthError::TokenExpired` if token validation fails.
pub struct AuthService {
    // fields
}

impl AuthService {
    /// Authenticates a user with email and password.
    ///
    /// # Arguments
    ///
    /// * `email` - User's email address
    /// * `password` - User's password (will be hashed before comparison)
    ///
    /// # Returns
    ///
    /// JWT access token valid for 24 hours
    ///
    /// # Errors
    ///
    /// Returns error if credentials are invalid or account is locked
    pub async fn login(&self, email: &str, password: &str) -> Result<String, AuthError> {
        // Implementation
    }
}
```

### Example 3: API Endpoint Documentation

```markdown
## Create User

Creates a new user account.

**Endpoint:** `POST /api/v1/users`

**Authentication:** Required (Admin role)

**Request Body:**
| Field | Type | Required | Description |
|-----------|--------|----------|--------------------------------|
| email | string | Yes | Valid email address |
| name | string | Yes | Full name (2-100 characters) |
| role | string | No | User role. Default: "member" |

**Example Request:**

    POST /api/v1/users
    Authorization: Bearer eyJhbGc...
    Content-Type: application/json

    {
      "email": "jane@example.com",
      "name": "Jane Smith",
      "role": "admin"
    }

**Success Response (201 Created):**

    {
      "id": "usr_abc123",
      "email": "jane@example.com",
      "name": "Jane Smith",
      "role": "admin",
      "createdAt": "2024-01-15T10:30:00Z"
    }

**Error Responses:**
| Status | Code | Description |
|--------|-------------------|---------------------------------|
| 400 | INVALID_EMAIL | Email format is invalid |
| 400 | NAME_TOO_SHORT | Name must be at least 2 chars |
| 409 | EMAIL_EXISTS | Email already registered |
| 403 | FORBIDDEN | Insufficient permissions |
```

### Example 4: Inline Comments (Good vs Bad)

```rust
// BAD: States the obvious
// Increment counter by 1
counter += 1;

// GOOD: Explains why
// Skip the first record as it's always the CSV header
counter += 1;

// BAD: Doesn't add value
// Check if user is valid
if user.is_valid() {

// GOOD: Explains non-obvious business rule
// Users created before 2020 don't have email verification enabled
if user.is_valid() {
```

## Deliverables

When creating documentation, deliver:

1. **Complete Coverage**: All public APIs, configuration options, and workflows documented
2. **Working Examples**: Code examples that compile and run successfully
3. **Clear Structure**: Logical organization with table of contents for long docs
4. **Up-to-date**: Documentation matches current code behavior
5. **Accessible**: Written at appropriate level for target audience
6. **Discoverable**: Proper file names, clear headings, good SEO keywords

## Common Pitfalls to Avoid

- Documenting implementation details that should be private
- Writing docs that duplicate what code already expresses
- Using vague language ("might", "usually", "sometimes")
- Forgetting to update docs when code changes
- Assuming reader knowledge without defining terms
- Writing documentation in passive voice
- Creating orphaned docs with no links to or from them
- Skipping error cases and edge conditions
