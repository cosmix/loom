---
name: api-documentation
description: Document REST APIs with OpenAPI/Swagger specifications, endpoint documentation, authentication, error handling, and SDK guides. Use for API reference docs, Swagger specs, and client library documentation. Triggers: api docs, openapi, swagger, endpoint documentation, rest api, api reference, sdk documentation, api specification, document api, api endpoints, request response examples, schema documentation, openapi 3.1, redoc, stoplight, postman collection, api explorer, interactive docs, api contract, api schema, swagger ui, authentication flows, rate limits.
---

# API Documentation

## Overview

Comprehensive API documentation skill covering OpenAPI/Swagger specifications, endpoint documentation, authentication flows, error handling, versioning strategies, and SDK/client documentation. Combines technical precision with clear, user-friendly writing to create documentation that developers can actually use.

## Instructions

### 1. Understand the API First

- Review all endpoints and their purposes
- Identify authentication mechanisms
- Understand request/response schemas
- Note error conditions and edge cases
- Check existing API patterns for consistency

### 2. Documentation Components

Every API should document:

1. **Authentication**: How to authenticate
2. **Base URL**: Environment-specific URLs
3. **Endpoints**: All available operations
4. **Schemas**: Request/response models
5. **Errors**: Error codes and handling
6. **Rate limits**: Throttling policies
7. **Versioning**: How versions work

## Best Practices

### Technical Accuracy
- Use consistent terminology throughout
- Keep schemas DRY with $ref
- Document all possible response codes
- Validate examples against schema
- Include proper JSON Schema constraints

### Clarity and Usability (from technical-writer expertise)
- Write for the developer audience: assume technical competence, skip patronizing explanations
- Lead with practical examples: show before explaining
- Use active voice and direct language
- Provide working curl examples for every endpoint
- Explain authentication with complete, copy-paste examples
- Document rate limits with concrete numbers and headers
- Surface common error scenarios prominently
- Include troubleshooting tips for frequent issues

### Interactive Documentation
- Design for auto-generated docs (Redoc, Swagger UI, Stoplight)
- Use operationId for stable client generation
- Tag endpoints logically for navigation
- Write descriptions that render well in both markdown and UI tools
- Include multiple request/response examples for different scenarios

## OpenAPI 3.1 Key Features

### Full JSON Schema Support
- Use JSON Schema 2020-12 vocabulary
- Support const, if/then/else, dependentSchemas
- Native oneOf/anyOf/allOf with discriminators

### Example Handling
- Single example: use `example` property
- Multiple examples: use `examples` object with named scenarios
- Examples in both schema definitions AND operation level

### Webhooks Documentation
```yaml
webhooks:
  userCreated:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/UserEvent'
```

### Content Negotiation
```yaml
content:
  application/json:
    schema: {...}
  application/xml:
    schema: {...}
  text/csv:
    schema:
      type: string
```

### Security Schemes
- OAuth2 with multiple flows
- OpenID Connect Discovery
- Mutual TLS
- Custom security schemes with extensions

## Examples

### OpenAPI/Swagger Specification

```yaml
openapi: 3.1.0
info:
  title: User Management API
  description: |
    API for managing users and their profiles.

    ## Authentication
    All endpoints require Bearer token authentication.
    Obtain a token via POST /auth/login.

    ## Rate Limits
    - Standard: 100 requests/minute
    - Authenticated: 1000 requests/minute
  version: 2.0.0
  contact:
    name: API Support
    email: api-support@example.com
    url: https://developer.example.com
  license:
    name: MIT
    url: https://opensource.org/licenses/MIT

servers:
  - url: https://api.example.com/v2
    description: Production
  - url: https://api.staging.example.com/v2
    description: Staging
  - url: http://localhost:3000/v2
    description: Local development

tags:
  - name: Users
    description: User management operations
  - name: Authentication
    description: Authentication and authorization

security:
  - BearerAuth: []

paths:
  /users:
    get:
      summary: List all users
      description: |
        Retrieve a paginated list of users.
        Results are sorted by creation date (newest first).
      operationId: listUsers
      tags:
        - Users
      parameters:
        - $ref: "#/components/parameters/PageParam"
        - $ref: "#/components/parameters/LimitParam"
        - name: status
          in: query
          description: Filter by user status
          schema:
            type: string
            enum: [active, inactive, pending]
        - name: search
          in: query
          description: Search by name or email
          schema:
            type: string
            minLength: 2
      responses:
        "200":
          description: Successful response
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/UserListResponse"
              example:
                data:
                  - id: "usr_123"
                    email: "john@example.com"
                    name: "John Doe"
                    status: "active"
                    createdAt: "2024-01-15T10:30:00Z"
                pagination:
                  page: 1
                  limit: 20
                  total: 150
                  hasMore: true
        "401":
          $ref: "#/components/responses/Unauthorized"
        "429":
          $ref: "#/components/responses/RateLimited"

    post:
      summary: Create a new user
      description: Create a new user account
      operationId: createUser
      tags:
        - Users
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/CreateUserRequest"
            examples:
              basic:
                summary: Basic user creation
                value:
                  email: "jane@example.com"
                  name: "Jane Smith"
                  password: "secureP@ssw0rd"
              withRole:
                summary: User with admin role
                value:
                  email: "admin@example.com"
                  name: "Admin User"
                  password: "secureP@ssw0rd"
                  role: "admin"
      responses:
        "201":
          description: User created successfully
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/UserResponse"
        "400":
          $ref: "#/components/responses/BadRequest"
        "409":
          description: User with this email already exists
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Error"
              example:
                error:
                  code: "USER_EXISTS"
                  message: "A user with this email already exists"

  /users/{userId}:
    get:
      summary: Get user by ID
      description: Retrieve a specific user by their unique identifier
      operationId: getUserById
      tags:
        - Users
      parameters:
        - $ref: "#/components/parameters/UserIdParam"
      responses:
        "200":
          description: Successful response
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/UserResponse"
        "404":
          $ref: "#/components/responses/NotFound"

    patch:
      summary: Update user
      description: Update user properties. Only provided fields are updated.
      operationId: updateUser
      tags:
        - Users
      parameters:
        - $ref: "#/components/parameters/UserIdParam"
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/UpdateUserRequest"
      responses:
        "200":
          description: User updated successfully
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/UserResponse"
        "400":
          $ref: "#/components/responses/BadRequest"
        "404":
          $ref: "#/components/responses/NotFound"

    delete:
      summary: Delete user
      description: Permanently delete a user account
      operationId: deleteUser
      tags:
        - Users
      parameters:
        - $ref: "#/components/parameters/UserIdParam"
      responses:
        "204":
          description: User deleted successfully
        "404":
          $ref: "#/components/responses/NotFound"

components:
  securitySchemes:
    BearerAuth:
      type: http
      scheme: bearer
      bearerFormat: JWT
      description: |
        JWT token obtained from POST /auth/login.
        Include in header: `Authorization: Bearer <token>`

    ApiKeyAuth:
      type: apiKey
      in: header
      name: X-API-Key
      description: API key for server-to-server authentication

  parameters:
    UserIdParam:
      name: userId
      in: path
      required: true
      description: Unique user identifier
      schema:
        type: string
        pattern: "^usr_[a-zA-Z0-9]+$"
      example: usr_123abc

    PageParam:
      name: page
      in: query
      description: Page number (1-indexed)
      schema:
        type: integer
        minimum: 1
        default: 1

    LimitParam:
      name: limit
      in: query
      description: Number of items per page
      schema:
        type: integer
        minimum: 1
        maximum: 100
        default: 20

  schemas:
    User:
      type: object
      required:
        - id
        - email
        - name
        - status
        - createdAt
      properties:
        id:
          type: string
          description: Unique identifier
          example: usr_123abc
        email:
          type: string
          format: email
          description: User's email address
          example: john@example.com
        name:
          type: string
          description: User's full name
          example: John Doe
        status:
          type: string
          enum: [active, inactive, pending]
          description: Account status
        role:
          type: string
          enum: [user, admin, moderator]
          default: user
        createdAt:
          type: string
          format: date-time
          description: Account creation timestamp
        updatedAt:
          type: string
          format: date-time
          description: Last update timestamp

    CreateUserRequest:
      type: object
      required:
        - email
        - name
        - password
      properties:
        email:
          type: string
          format: email
        name:
          type: string
          minLength: 2
          maxLength: 100
        password:
          type: string
          format: password
          minLength: 8
          description: Must contain uppercase, lowercase, number, and special character
        role:
          type: string
          enum: [user, admin, moderator]
          default: user

    UpdateUserRequest:
      type: object
      minProperties: 1
      properties:
        name:
          type: string
          minLength: 2
          maxLength: 100
        status:
          type: string
          enum: [active, inactive]

    UserResponse:
      type: object
      properties:
        data:
          $ref: "#/components/schemas/User"

    UserListResponse:
      type: object
      properties:
        data:
          type: array
          items:
            $ref: "#/components/schemas/User"
        pagination:
          $ref: "#/components/schemas/Pagination"

    Pagination:
      type: object
      properties:
        page:
          type: integer
        limit:
          type: integer
        total:
          type: integer
        hasMore:
          type: boolean

    Error:
      type: object
      properties:
        error:
          type: object
          properties:
            code:
              type: string
              description: Machine-readable error code
            message:
              type: string
              description: Human-readable error message
            details:
              type: array
              items:
                type: object
                properties:
                  field:
                    type: string
                  message:
                    type: string

  responses:
    BadRequest:
      description: Invalid request parameters
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/Error"
          example:
            error:
              code: "VALIDATION_ERROR"
              message: "Request validation failed"
              details:
                - field: "email"
                  message: "Must be a valid email address"

    Unauthorized:
      description: Authentication required
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/Error"
          example:
            error:
              code: "UNAUTHORIZED"
              message: "Invalid or missing authentication token"

    NotFound:
      description: Resource not found
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/Error"
          example:
            error:
              code: "NOT_FOUND"
              message: "The requested resource was not found"

    RateLimited:
      description: Rate limit exceeded
      headers:
        X-RateLimit-Limit:
          schema:
            type: integer
          description: Request limit per minute
        X-RateLimit-Remaining:
          schema:
            type: integer
          description: Remaining requests
        X-RateLimit-Reset:
          schema:
            type: integer
          description: Unix timestamp when limit resets
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/Error"
          example:
            error:
              code: "RATE_LIMITED"
              message: "Too many requests. Please retry after 60 seconds."
```

### Endpoint Documentation (Markdown)

```markdown
## Create User

Create a new user account in the system.

**Endpoint:** `POST /users`

**Authentication:** Required (Bearer token)

### Request

#### Headers

| Header        | Required | Description                           |
| ------------- | -------- | ------------------------------------- |
| Authorization | Yes      | Bearer token                          |
| Content-Type  | Yes      | `application/json`                    |
| X-Request-Id  | No       | Unique request identifier for tracing |

#### Body Parameters

| Parameter | Type   | Required | Description                                                                     |
| --------- | ------ | -------- | ------------------------------------------------------------------------------- |
| email     | string | Yes      | Valid email address                                                             |
| name      | string | Yes      | Full name (2-100 characters)                                                    |
| password  | string | Yes      | Password (min 8 chars, must include uppercase, lowercase, number, special char) |
| role      | string | No       | User role: `user`, `admin`, `moderator`. Default: `user`                        |

#### Example Request

\`\`\`bash
curl -X POST https://api.example.com/v2/users \
 -H "Authorization: Bearer eyJhbG..." \
 -H "Content-Type: application/json" \
 -d '{
"email": "jane@example.com",
"name": "Jane Smith",
"password": "SecureP@ss123"
}'
\`\`\`

### Response

#### Success Response (201 Created)

\`\`\`json
{
"data": {
"id": "usr_abc123",
"email": "jane@example.com",
"name": "Jane Smith",
"status": "pending",
"role": "user",
"createdAt": "2024-01-15T10:30:00Z"
}
}
\`\`\`

#### Error Responses

| Status | Code             | Description              |
| ------ | ---------------- | ------------------------ |
| 400    | VALIDATION_ERROR | Invalid request body     |
| 401    | UNAUTHORIZED     | Missing or invalid token |
| 409    | USER_EXISTS      | Email already registered |
| 429    | RATE_LIMITED     | Too many requests        |

**400 Bad Request:**
\`\`\`json
{
"error": {
"code": "VALIDATION_ERROR",
"message": "Request validation failed",
"details": [
{
"field": "password",
"message": "Password must be at least 8 characters"
}
]
}
}
\`\`\`
```

### Authentication Documentation

```markdown
# Authentication

## Overview

The API supports two authentication methods:

1. **Bearer Token (JWT)** - For user-facing applications
2. **API Key** - For server-to-server integrations

## Bearer Token Authentication

### Obtaining a Token

\`\`\`bash
POST /auth/login
Content-Type: application/json

{
"email": "user@example.com",
"password": "your-password"
}
\`\`\`

**Response:**
\`\`\`json
{
"accessToken": "eyJhbGciOiJIUzI1NiIs...",
"refreshToken": "dGhpcyBpcyBhIHJlZnJl...",
"expiresIn": 3600,
"tokenType": "Bearer"
}
\`\`\`

### Using the Token

Include the token in the Authorization header:

\`\`\`
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
\`\`\`

### Token Lifecycle

| Token         | Lifetime | Usage                   |
| ------------- | -------- | ----------------------- |
| Access Token  | 1 hour   | API requests            |
| Refresh Token | 30 days  | Obtain new access token |

### Refreshing Tokens

\`\`\`bash
POST /auth/refresh
Content-Type: application/json

{
"refreshToken": "dGhpcyBpcyBhIHJlZnJl..."
}
\`\`\`

## API Key Authentication

For server-to-server integrations, use API key authentication.

### Creating an API Key

1. Go to Dashboard > Settings > API Keys
2. Click "Create New Key"
3. Select permissions and expiration
4. Copy and securely store the key

### Using the API Key

Include in the `X-API-Key` header:

\`\`\`
X-API-Key: sk_live_abc123...
\`\`\`

### API Key Best Practices

- Never expose keys in client-side code
- Rotate keys regularly (every 90 days)
- Use separate keys for each environment
- Apply minimum required permissions
```

### Error Response Documentation

```markdown
# Error Handling

## Error Response Format

All errors follow a consistent format:

\`\`\`json
{
"error": {
"code": "ERROR_CODE",
"message": "Human-readable description",
"details": [
{
"field": "fieldName",
"message": "Field-specific error"
}
],
"requestId": "req_abc123"
}
}
\`\`\`

## HTTP Status Codes

| Status | Meaning           | When Used                               |
| ------ | ----------------- | --------------------------------------- |
| 200    | OK                | Successful GET, PUT, PATCH              |
| 201    | Created           | Successful POST creating resource       |
| 204    | No Content        | Successful DELETE                       |
| 400    | Bad Request       | Invalid request syntax or parameters    |
| 401    | Unauthorized      | Missing or invalid authentication       |
| 403    | Forbidden         | Valid auth but insufficient permissions |
| 404    | Not Found         | Resource doesn't exist                  |
| 409    | Conflict          | Resource state conflict                 |
| 422    | Unprocessable     | Valid syntax but semantic errors        |
| 429    | Too Many Requests | Rate limit exceeded                     |
| 500    | Internal Error    | Server-side error                       |

## Common Error Codes

### Authentication Errors

| Code               | HTTP Status | Description            | Resolution                   |
| ------------------ | ----------- | ---------------------- | ---------------------------- |
| UNAUTHORIZED       | 401         | No token provided      | Include Authorization header |
| TOKEN_EXPIRED      | 401         | Token has expired      | Refresh your access token    |
| TOKEN_INVALID      | 401         | Token is malformed     | Obtain a new token           |
| INSUFFICIENT_SCOPE | 403         | Token lacks permission | Request with proper scopes   |

### Validation Errors

| Code             | HTTP Status | Description               | Resolution                          |
| ---------------- | ----------- | ------------------------- | ----------------------------------- |
| VALIDATION_ERROR | 400         | Request failed validation | Check `details` for specific fields |
| INVALID_JSON     | 400         | Malformed JSON body       | Verify JSON syntax                  |
| MISSING_FIELD    | 400         | Required field missing    | Include all required fields         |

### Resource Errors

| Code            | HTTP Status | Description            | Resolution             |
| --------------- | ----------- | ---------------------- | ---------------------- |
| NOT_FOUND       | 404         | Resource doesn't exist | Verify resource ID     |
| ALREADY_EXISTS  | 409         | Duplicate resource     | Use unique identifiers |
| RESOURCE_LOCKED | 423         | Resource is locked     | Wait and retry         |

## Handling Errors in Code

### JavaScript/TypeScript

\`\`\`typescript
try {
const response = await api.createUser(userData);
} catch (error) {
if (error.status === 401) {
await refreshToken();
return retry();
}

if (error.status === 429) {
const retryAfter = error.headers['retry-after'];
await sleep(retryAfter \* 1000);
return retry();
}

if (error.body?.error?.code === 'VALIDATION_ERROR') {
const fieldErrors = error.body.error.details;
displayFieldErrors(fieldErrors);
}
}
\`\`\`
```

### Versioning Documentation

```markdown
# API Versioning

## Version Format

The API uses URL path versioning:

\`\`\`
https://api.example.com/v{major}/resource
\`\`\`

Current version: **v2**

## Supported Versions

| Version | Status     | Sunset Date |
| ------- | ---------- | ----------- |
| v2      | Current    | -           |
| v1      | Deprecated | 2024-12-31  |

## Version Lifecycle

1. **Current**: Actively developed and supported
2. **Deprecated**: Supported but no new features
3. **Sunset**: Read-only access for 90 days
4. **Retired**: No longer accessible

## Breaking vs Non-Breaking Changes

### Non-Breaking (No version bump)

- Adding new endpoints
- Adding optional parameters
- Adding new response fields
- Adding new enum values

### Breaking (Major version bump)

- Removing endpoints
- Removing or renaming fields
- Changing field types
- Changing authentication method
- Changing error format

## Migration Guide: v1 to v2

### Authentication Change

**v1:** API Key in query parameter
\`\`\`
GET /v1/users?api_key=abc123
\`\`\`

**v2:** Bearer token in header
\`\`\`
GET /v2/users
Authorization: Bearer eyJhbG...
\`\`\`

### Response Format Change

**v1:** Flat response
\`\`\`json
{
"id": "123",
"name": "John"
}
\`\`\`

**v2:** Wrapped response
\`\`\`json
{
"data": {
"id": "usr_123",
"name": "John"
}
}
\`\`\`
```

### SDK Documentation

```markdown
# JavaScript SDK

## Installation

\`\`\`bash
npm install @example/api-client
\`\`\`

## Quick Start

\`\`\`javascript
import { ApiClient } from '@example/api-client';

const client = new ApiClient({
apiKey: process.env.API_KEY,
environment: 'production' // or 'sandbox'
});

// Create a user
const user = await client.users.create({
email: 'jane@example.com',
name: 'Jane Smith'
});

console.log(user.id); // usr_abc123
\`\`\`

## Configuration

\`\`\`javascript
const client = new ApiClient({
// Required
apiKey: 'sk*live*...',

// Optional
environment: 'production', // 'production' | 'sandbox'
timeout: 30000, // Request timeout in ms
retries: 3, // Auto-retry failed requests
logger: console, // Custom logger
});
\`\`\`

## Resources

### Users

\`\`\`javascript
// List users with pagination
const { data, pagination } = await client.users.list({
page: 1,
limit: 20,
status: 'active'
});

// Get single user
const user = await client.users.get('usr_123');

// Create user
const newUser = await client.users.create({
email: 'new@example.com',
name: 'New User'
});

// Update user
const updated = await client.users.update('usr_123', {
name: 'Updated Name'
});

// Delete user
await client.users.delete('usr_123');
\`\`\`

## Error Handling

\`\`\`javascript
import { ApiError, ValidationError, RateLimitError } from '@example/api-client';

try {
await client.users.create({ email: 'invalid' });
} catch (error) {
if (error instanceof ValidationError) {
console.log(error.details); // Field-level errors
} else if (error instanceof RateLimitError) {
console.log(`Retry after ${error.retryAfter} seconds`);
} else if (error instanceof ApiError) {
console.log(error.code, error.message);
}
}
\`\`\`

## TypeScript Support

The SDK includes full TypeScript definitions:

\`\`\`typescript
import { ApiClient, User, CreateUserParams } from '@example/api-client';

const params: CreateUserParams = {
email: 'typed@example.com',
name: 'Typed User'
};

const user: User = await client.users.create(params);
\`\`\`
```

## Documentation Tooling

### Interactive Documentation Generators

**Redoc** - Clean, three-panel layout with search
```bash
npx @redocly/cli build-docs openapi.yaml --output docs.html
```

**Swagger UI** - Try-it-out functionality, widely recognized
```bash
docker run -p 80:8080 -e SWAGGER_JSON=/api/openapi.yaml -v $(pwd):/api swaggerapi/swagger-ui
```

**Stoplight Elements** - Embeddable web components
```html
<script src="https://unpkg.com/@stoplight/elements/web-components.min.js"></script>
<elements-api apiDescriptionUrl="./openapi.yaml" router="hash"/>
```

### Validation and Linting

**Redocly CLI** - Lint OpenAPI specs
```bash
npx @redocly/cli lint openapi.yaml
npx @redocly/cli bundle openapi.yaml --output bundled.yaml
```

**Spectral** - Flexible linting with custom rules
```bash
spectral lint openapi.yaml --ruleset .spectral.yaml
```

### Client Generation

**OpenAPI Generator** - Multi-language client/server generation
```bash
openapi-generator-cli generate -i openapi.yaml -g typescript-fetch -o ./client
```

**Postman Collection Export**
```bash
npm install -g openapi-to-postmanv2
openapi2postmanv2 -s openapi.yaml -o collection.json
```

### Testing and Mocking

**Prism** - Mock server from OpenAPI spec
```bash
prism mock openapi.yaml
# Returns example responses from your spec
```

**Dredd** - Test API against OpenAPI contract
```bash
dredd openapi.yaml http://localhost:3000
```

## Documentation Patterns

### Progressive Disclosure
Structure docs from quick start to advanced topics:
1. Authentication quick start (single example)
2. Common use cases (recipes)
3. Complete endpoint reference
4. Advanced topics (webhooks, pagination strategies)

### Code Examples Strategy
- Provide examples in multiple languages (curl, JavaScript, Python, Go)
- Use realistic data (not foo/bar)
- Show complete working examples, not fragments
- Include error handling in examples

### API Explorer Integration
Include "Try It" functionality:
- Pre-fill authentication from environment
- Provide example payloads
- Show actual requests/responses
- Support environment switching (sandbox/production)

### Versioning Communication
- Announce deprecations 6+ months in advance
- Provide migration guides with side-by-side comparisons
- Include deprecation headers in API responses
- Maintain changelog with breaking/non-breaking labels
