---
name: technical-writing
description: Professional technical documentation writing for software projects including README files, user guides, migration guides, changelogs, API documentation, error messages, release notes, and developer documentation. Covers documentation style, tone, voice, clarity, conciseness, and audience-appropriate writing. Triggers: technical writing, documentation, docs, readme, guide, tutorial, changelog, migration guide, write docs, document code, documentation style, writing guide, tone, voice, clarity, concise, user documentation, developer documentation, API writing, API docs, error messages, release notes, technical communication, writing for developers, documentation standards.
---

# Technical Writing

## Overview

Comprehensive technical writing skill covering all software documentation needs: user guides, developer documentation, API references, error messages, release notes, migration guides, and README files. Provides expertise in documentation style, tone, clarity, and audience-appropriate communication.

## Instructions

### 1. Understand Before Documenting

- **Read the code first**: Never document without understanding the implementation
- **Identify the purpose**: What problem does this code solve?
- **Trace the flow**: Understand how components interact
- **Note edge cases**: Document limitations and constraints
- **Check existing docs**: Review current documentation for style and gaps

### 2. Know Your Audience

- **Developers**: Focus on API details, code examples, technical depth, implementation patterns
- **End users**: Focus on tasks, outcomes, minimal technical jargon, clear instructions
- **Operators/DevOps**: Focus on deployment, configuration, monitoring, troubleshooting
- **New team members**: Focus on onboarding, architecture overview, conceptual understanding
- **Open source contributors**: Focus on contribution workflow, code standards, testing requirements

### 3. Structure Documents Effectively

Standard documentation structure:

```
1. Overview/Introduction
   - What is this?
   - Why does it exist?
   - Who is it for?

2. Getting Started
   - Prerequisites
   - Installation
   - Quick start example

3. Core Concepts
   - Key terminology
   - Architecture overview
   - Mental models

4. Usage Guide
   - Common tasks
   - Configuration options
   - Best practices

5. API Reference
   - Methods/endpoints
   - Parameters
   - Return values
   - Examples

6. Troubleshooting
   - Common issues
   - Error messages
   - FAQ

7. Contributing (if applicable)
   - Development setup
   - Code style
   - PR process
```

### 4. Writing Process

1. **Outline first**: Create structure before prose
2. **Write draft**: Get content down without perfecting
3. **Add examples**: Include working code snippets
4. **Review for clarity**: Simplify complex sentences
5. **Check consistency**: Verify terminology and style
6. **Test examples**: Ensure code samples work

## Best Practices

### Prose Style

- **Active voice**: "The function returns" not "The value is returned by"
- **Conciseness**: Remove unnecessary words. "Use X" not "You can use X if you want"
- **Specificity**: "Returns a string" not "Returns the result"
- **Present tense**: For descriptions and current behavior
- **Imperative mood**: For instructions: "Run the command" not "You should run"
- **Parallel structure**: Keep list items grammatically consistent
- **Avoid hedging**: "This feature does X" not "This feature might do X"
- **Front-load information**: Lead with the most important point

### README Best Practices

```markdown
# Project Name

Brief one-line description.

## Features

- Key feature 1
- Key feature 2

## Installation

\`\`\`bash
npm install package-name
\`\`\`

## Quick Start

\`\`\`javascript
import { feature } from 'package-name';

const result = feature.doSomething();
\`\`\`

## Documentation

Link to full docs.

## Contributing

Link to contributing guide.

## License

MIT
```

### Changelog Best Practices

Follow [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
# Changelog

## [Unreleased]

## [1.2.0] - 2024-01-15

### Added

- New feature X for doing Y

### Changed

- Updated dependency Z to version 2.0

### Fixed

- Bug where A caused B

### Removed

- Deprecated method `oldMethod()`
```

### Migration Guide Best Practices

```markdown
# Migration Guide: v1.x to v2.0

## Overview

Brief description of why migration is needed.

## Breaking Changes

### Change 1: New Authentication

**Before (v1.x):**
\`\`\`javascript
client.auth(apiKey);
\`\`\`

**After (v2.0):**
\`\`\`javascript
client.authenticate({ key: apiKey, type: 'bearer' });
\`\`\`

## Step-by-Step Migration

1. Update package to v2.0
2. Replace auth calls (see above)
3. Update configuration file format
4. Test your integration

## Deprecation Timeline

- v2.0: Old methods deprecated with warnings
- v3.0: Old methods removed
```

### Error Message Best Practices

Error messages should be actionable, specific, and respectful:

**Structure:**
1. What happened (the error)
2. Why it happened (the cause)
3. What to do (the solution)

**Good error messages:**
```
Error: Configuration file not found at '/config.json'

The application looks for config.json in the current directory.

To fix this:
- Create a config.json file in your project root, OR
- Specify a custom path with --config flag

Example: myapp --config /path/to/config.json
```

**Bad error messages:**
```
Error: null reference exception
Invalid input
Something went wrong
```

**Guidelines:**
- Be specific about what failed
- Avoid technical jargon for user-facing errors
- Provide actionable next steps
- Include examples when helpful
- Don't blame the user ("You made a mistake")
- Use neutral tone ("The file was not found" not "You didn't provide a file")

### Release Notes Best Practices

Release notes communicate changes to users in a scannable, prioritized format:

```markdown
# Release v2.5.0 - January 2026

## Highlights

Brief paragraph summarizing the most important changes and their impact.

## New Features

### Feature Name
Brief description of what the feature does and why users care.

Example usage:
\`\`\`bash
command --new-flag value
\`\`\`

**Who should use this**: Teams managing large codebases

### Another Feature
Description and example.

## Improvements

- **Performance**: Database queries are now 3x faster for large datasets
- **UX**: The dashboard now loads incrementally for better perceived performance
- **API**: Added support for batch operations in the REST API

## Bug Fixes

- Fixed crash when processing files with Unicode characters
- Fixed incorrect totals in reports when timezone spans midnight
- Fixed memory leak in long-running background jobs

## Breaking Changes

### Changed Authentication Flow

**Impact**: All API clients must update authentication code

**Before:**
\`\`\`javascript
client.authenticate(token);
\`\`\`

**After:**
\`\`\`javascript
client.authenticate({ token, type: 'bearer' });
\`\`\`

**Migration deadline**: v3.0 (6 months)

## Deprecations

- `oldMethod()` is deprecated. Use `newMethod()` instead.
- Support for Node 14 will be removed in v3.0

## Upgrade Instructions

1. Backup your data
2. Update to v2.5.0: `npm install package@2.5.0`
3. Run migration script: `npm run migrate`
4. Update authentication code (see Breaking Changes)
5. Test your integration

## Known Issues

- Dashboard may be slow on Safari 15 (fix planned for v2.5.1)
- Export to PDF not working on Windows (workaround: export to CSV)
```

**Guidelines:**
- Lead with user impact, not implementation details
- Group by type (features, fixes, breaking changes)
- Provide migration paths for breaking changes
- Include code examples for changed APIs
- Specify version numbers and timelines
- Acknowledge known issues honestly

### Writing for Developers

Developer documentation requires technical depth with clarity:

**API Documentation Structure:**
```markdown
## MethodName

Brief one-line description of what this method does.

### Signature

\`\`\`typescript
functionName(param1: Type1, options?: Options): ReturnType
\`\`\`

### Parameters

| Parameter | Type     | Required | Description                    |
| --------- | -------- | -------- | ------------------------------ |
| `param1`  | `string` | Yes      | The primary input value        |
| `options` | `Object` | No       | Configuration options (below)  |

#### Options

| Option      | Type      | Default | Description                      |
| ----------- | --------- | ------- | -------------------------------- |
| `timeout`   | `number`  | `5000`  | Request timeout in milliseconds  |
| `retries`   | `number`  | `3`     | Number of retry attempts         |

### Returns

Returns a `Promise<Result>` that resolves with:

| Field     | Type     | Description                |
| --------- | -------- | -------------------------- |
| `success` | `boolean`| Whether operation succeeded|
| `data`    | `any`    | Response data if successful|
| `error`   | `string` | Error message if failed    |

### Errors

| Error Code | Condition                    | Resolution                  |
| ---------- | ---------------------------- | --------------------------- |
| `TIMEOUT`  | Request exceeds timeout      | Increase timeout value      |
| `INVALID`  | Parameter validation failed  | Check parameter types       |

### Examples

Basic usage:
\`\`\`typescript
const result = await functionName('input-value');
if (result.success) {
  console.log(result.data);
}
\`\`\`

With options:
\`\`\`typescript
const result = await functionName('input-value', {
  timeout: 10000,
  retries: 5
});
\`\`\`

Error handling:
\`\`\`typescript
try {
  const result = await functionName('input-value');
} catch (error) {
  if (error.code === 'TIMEOUT') {
    // Handle timeout
  }
}
\`\`\`

### Notes

- This method is rate-limited to 100 calls per minute
- Requires authentication with API key
- Available since version 2.0
```

**Architecture Documentation:**
- Include diagrams (ASCII art, mermaid, or references to images)
- Explain the "why" behind design decisions
- Document trade-offs and alternatives considered
- Provide context for future maintainers
- Link to relevant ADRs (Architecture Decision Records)

**Code Comments (when needed):**
- Explain "why" not "what" (code shows what)
- Document non-obvious behavior
- Explain workarounds or hacks
- Reference issues/tickets for context
- Keep comments up-to-date with code changes

## Examples

### Example: Function Documentation

```javascript
/**
 * Calculates the total price including tax and discounts.
 *
 * @param {number} basePrice - The original price before adjustments
 * @param {Object} options - Calculation options
 * @param {number} [options.taxRate=0.1] - Tax rate as decimal (0.1 = 10%)
 * @param {number} [options.discount=0] - Discount amount in currency units
 * @returns {number} Final price rounded to 2 decimal places
 * @throws {Error} If basePrice is negative
 *
 * @example
 * // Basic usage
 * calculateTotal(100);
 * // Returns: 110.00
 *
 * @example
 * // With discount
 * calculateTotal(100, { discount: 20 });
 * // Returns: 88.00
 */
function calculateTotal(basePrice, options = {}) {
  // implementation
}
```

### Example: CLI Documentation

```markdown
## Usage

\`\`\`
myapp <command> [options]
\`\`\`

### Commands

| Command | Description              |
| ------- | ------------------------ |
| `init`  | Initialize a new project |
| `build` | Build the project        |
| `serve` | Start development server |

### Options

| Option      | Alias | Description           | Default         |
| ----------- | ----- | --------------------- | --------------- |
| `--config`  | `-c`  | Path to config file   | `./config.json` |
| `--verbose` | `-v`  | Enable verbose output | `false`         |
| `--port`    | `-p`  | Server port           | `3000`          |

### Examples

Initialize a new project:
\`\`\`bash
myapp init my-project
\`\`\`

Build with custom config:
\`\`\`bash
myapp build -c ./custom-config.json
\`\`\`
```
