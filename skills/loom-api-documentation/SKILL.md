---
name: loom-api-documentation
description: Document REST APIs with OpenAPI/Swagger specifications, endpoint references, authentication flows, error handling, and SDK guides. Use for API reference docs, Swagger specs, interactive explorers, and client library documentation.
triggers:
  - api docs
  - openapi
  - swagger
  - endpoint documentation
  - rest api
  - api reference
  - sdk documentation
  - api specification
  - document api
  - api endpoints
  - request response examples
  - schema documentation
  - openapi 3.1
  - redoc
  - stoplight
  - postman collection
  - api explorer
  - interactive docs
  - api contract
  - api schema
  - swagger ui
  - authentication flows
  - rate limits
  - contract testing
  - spec-first
---

# API Documentation

## Overview

Produce API docs developers can actually use: an accurate OpenAPI spec as the source of truth, plus reference/auth/error/versioning guides generated or kept in sync with it. Correctness and drift-prevention matter more than prose.

## What every API must document

Auth · base URLs per environment · every endpoint + operation · request/response schemas · **all** response codes (incl. errors) · rate limits (with headers) · pagination · versioning/deprecation policy.

## Spec-first vs code-first (choose deliberately)

| Approach | How | Drift risk | Use when |
| -------- | --- | ---------- | -------- |
| **Spec-first** | Hand-write OpenAPI, generate server stubs + clients + mocks | Runtime can diverge from spec unless validated | New APIs, contract negotiated across teams, mock-driven frontend |
| **Code-first** | Annotate handlers; framework emits spec (FastAPI, springdoc, drf-spectacular, tsoa) | Spec stays close to code, but annotations can lie | Existing codebase, small team, code is the truth |

Either way, **enforce the contract in CI** (lint + validate examples + breaking-change diff). Docs that aren't tested against the running API are fiction.

## OpenAPI 3.1 — what changed from 3.0 (get these right)

- **Fully aligned with JSON Schema 2020-12.** A schema is now a valid JSON Schema; you can set `jsonSchemaDialect` and use `$schema` per-schema.
- **`nullable: true` is GONE.** Use a type array: `type: [string, "null"]`.
- **Type can be an array**: `type: [string, integer]`.
- **`exclusiveMinimum`/`exclusiveMaximum` are numbers**, not booleans (draft-4 behavior removed).
- **Top-level `webhooks`** describe events the API *sends* (see below).
- **Examples split by object**: Schema Objects use JSON Schema's `examples` (an **array**); Media Type / Parameter Objects use `example` (singular) or `examples` (a **map of named Example Objects** with `summary`/`value`). Don't confuse the two.
- `info.license.identifier` accepts an **SPDX** id (e.g., `MIT`) instead of a URL.
- `$ref` may now sit alongside sibling keywords (e.g., `description`).

⚠ Tooling lag: Swagger UI / some generators still have partial 3.1 support. Verify your renderer and codegen handle 3.1 before committing to `type: [..., "null"]` everywhere.

## Documentation quality rules

- Write for competent developers: skip patronizing basics; lead with a working example, then explain.
- Keep schemas DRY with `$ref`; reuse `parameters`, `responses`, `securitySchemes` from `components`.
- **Every operation needs a unique `operationId`** — it becomes the generated client's method name. Renaming it is a breaking change for SDK users.
- Tag endpoints for navigation; realistic example data (not `foo`/`bar`); document rate limits with concrete numbers **and** the exact headers.
- Validate every example against its schema (Redocly/Spectral catch this).

## Examples

### OpenAPI 3.1 spec (trimmed to the load-bearing shapes)

```yaml
openapi: 3.1.0
info:
  title: User Management API
  version: 2.0.0
  license: { name: MIT, identifier: MIT }   # 3.1 SPDX identifier
servers:
  - { url: https://api.example.com/v2, description: Production }
  - { url: https://api.staging.example.com/v2, description: Staging }
security:
  - BearerAuth: []
tags:
  - { name: Users, description: User management }

paths:
  /users:
    get:
      summary: List users
      operationId: listUsers          # stable → SDK method name
      tags: [Users]
      parameters:
        - $ref: "#/components/parameters/LimitParam"
        - name: status
          in: query
          schema: { type: string, enum: [active, inactive, pending] }
      responses:
        "200":
          description: OK
          content:
            application/json:
              schema: { $ref: "#/components/schemas/UserListResponse" }
              examples:              # media-type: MAP of named Example Objects
                page1:
                  summary: First page
                  value:
                    data: [{ id: usr_123, email: john@example.com, status: active }]
                    pagination: { limit: 20, next_cursor: "eyJpZCI6MTIzfQ", has_more: true }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/RateLimited" }
    post:
      summary: Create user
      operationId: createUser
      tags: [Users]
      parameters:
        - name: Idempotency-Key
          in: header
          schema: { type: string }
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/CreateUserRequest" }
      responses:
        "201": { description: Created, content: { application/json: { schema: { $ref: "#/components/schemas/User" } } } }
        "409":
          description: Email already exists
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Problem" }

webhooks:                            # 3.1: events the API SENDS
  userCreated:
    post:
      requestBody:
        content:
          application/json:
            schema: { $ref: "#/components/schemas/User" }
      responses:
        "200": { description: Receiver acknowledged }

components:
  securitySchemes:
    BearerAuth: { type: http, scheme: bearer, bearerFormat: JWT }
    ApiKeyAuth: { type: apiKey, in: header, name: X-API-Key }
    OAuth2:
      type: oauth2
      flows:
        authorizationCode:
          authorizationUrl: https://auth.example.com/authorize
          tokenUrl: https://auth.example.com/token
          scopes: { "users:read": Read users, "users:write": Manage users }
  parameters:
    LimitParam:
      name: limit
      in: query
      schema: { type: integer, minimum: 1, maximum: 100, default: 20 }
  schemas:
    User:
      type: object
      required: [id, email, status]
      properties:
        id: { type: string, example: usr_123 }
        email: { type: string, format: email }
        status: { type: string, enum: [active, inactive, pending] }
        deletedAt: { type: [string, "null"], format: date-time }   # 3.1 nullable
    CreateUserRequest:
      type: object
      required: [email, name, password]
      properties:
        email: { type: string, format: email }
        name: { type: string, minLength: 2, maxLength: 100 }
        password: { type: string, format: password, minLength: 8 }
    UserListResponse:
      type: object
      properties:
        data: { type: array, items: { $ref: "#/components/schemas/User" } }
        pagination:
          type: object
          properties:
            limit: { type: integer }
            next_cursor: { type: [string, "null"] }
            has_more: { type: boolean }
    Problem:                          # RFC 9457 application/problem+json
      type: object
      properties:
        type: { type: string, format: uri }
        title: { type: string }
        status: { type: integer }
        detail: { type: string }
        errors:
          type: array
          items:
            type: object
            properties:
              field: { type: string }
              code: { type: string }
              message: { type: string }
  responses:
    Unauthorized:
      description: Authentication required
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Problem" }
    RateLimited:
      description: Rate limit exceeded
      headers:
        X-RateLimit-Limit: { schema: { type: integer }, description: Requests per window }
        X-RateLimit-Remaining: { schema: { type: integer } }
        X-RateLimit-Reset: { schema: { type: integer }, description: Unix epoch when window resets }
        Retry-After: { schema: { type: integer }, description: Seconds to wait }
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Problem" }
```

### Endpoint reference (Markdown template)

````markdown
## Create User — `POST /users`

Auth: Bearer token. Idempotent via `Idempotency-Key` header.

**Body**

| Field    | Type   | Req | Notes                                             |
| -------- | ------ | --- | ------------------------------------------------- |
| email    | string | yes | Valid email                                       |
| name     | string | yes | 2–100 chars                                        |
| password | string | yes | ≥8 chars; upper+lower+digit+symbol                |

```bash
curl -X POST https://api.example.com/v2/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: 9f1c...-once" \
  -d '{"email":"jane@example.com","name":"Jane Smith","password":"SecureP@ss123"}'
```

**201 Created** → `{ "id": "usr_abc123", "email": "jane@example.com", "status": "pending" }`

| Status | Code             | Meaning                  |
| ------ | ---------------- | ------------------------ |
| 401    | UNAUTHORIZED     | Missing/invalid token    |
| 409    | USER_EXISTS      | Email already registered |
| 422    | VALIDATION_ERROR | See `errors[]` per field |
| 429    | RATE_LIMITED     | Honor `Retry-After`      |
````

### Auth flows (document the full lifecycle, not just the header)

````markdown
## Bearer (JWT)

`POST /auth/login` → `{ "accessToken": "...", "refreshToken": "...", "expiresIn": 3600 }`
Send `Authorization: Bearer <accessToken>`. Access token 1h, refresh 30d; `POST /auth/refresh` to renew.

## API key (server-to-server)

`X-API-Key: sk_live_...`. Never ship keys client-side; scope minimally; rotate ≤90 days; one key per environment.
````

### Documenting errors, rate limits, versioning consistently

Reference the RFC 9457 `Problem` schema for **every** error response; maintain one canonical error-code table:

| Code | HTTP | Meaning | Client action |
| ---- | ---- | ------- | ------------- |
| UNAUTHORIZED | 401 | No/invalid token | Re-auth |
| INSUFFICIENT_SCOPE | 403 | Token lacks scope | Request scopes |
| VALIDATION_ERROR | 422 | Field validation failed | Inspect `errors[]` |
| NOT_FOUND | 404 | Missing resource | Verify id |
| ALREADY_EXISTS | 409 | Duplicate/conflict | Use unique key |
| RATE_LIMITED | 429 | Throttled | Wait `Retry-After` |

Rate limits: always document the window, the limit, and the `X-RateLimit-*` + `Retry-After` headers. Versioning: publish supported versions + sunset dates, classify breaking vs non-breaking, and give side-by-side migration examples plus `Deprecation`/`Sunset` response headers.

## Tooling

| Job | Tool | Command |
| --- | ---- | ------- |
| Render (3-panel) | Redoc | `npx @redocly/cli build-docs openapi.yaml -o docs.html` |
| Render (try-it) | Swagger UI | `docker run -p 80:8080 -e SWAGGER_JSON=/api/openapi.yaml -v $(pwd):/api swaggerapi/swagger-ui` |
| Embed | Stoplight Elements | `<elements-api apiDescriptionUrl="./openapi.yaml" router="hash" />` |
| Lint | Spectral / Redocly | `spectral lint openapi.yaml` · `npx @redocly/cli lint openapi.yaml` |
| Bundle | Redocly | `npx @redocly/cli bundle openapi.yaml -o bundled.yaml` |
| Client gen | OpenAPI Generator | `openapi-generator-cli generate -i openapi.yaml -g typescript-fetch -o ./client` |
| Postman | openapi-to-postmanv2 | `openapi2postmanv2 -s openapi.yaml -o collection.json` |
| Mock | Prism | `prism mock openapi.yaml` |
| Contract test | Dredd / Schemathesis | `dredd openapi.yaml http://localhost:3000` · `schemathesis run openapi.yaml` |
| Breaking-change diff | oasdiff | `oasdiff breaking old.yaml new.yaml` |

## Keeping docs in sync with code (the real problem)

Docs rot the moment they're decoupled from the running service. Enforce sync mechanically:

- **Runtime validation** — proxy requests/responses through the spec (`express-openapi-validator`, Prism proxy) in dev/staging; a mismatch fails the build.
- **Contract tests in CI** — Dredd (example-driven) or Schemathesis (property-based fuzzing derived from the schema) run against the real API; Schemathesis catches undocumented 500s and schema violations you'd never write by hand.
- **Breaking-change gate** — `oasdiff breaking` (or openapi-diff) on every PR blocks silent contract breaks.
- **Lint gate** — Spectral ruleset enforces house style (descriptions present, `operationId` unique, examples valid, error responses documented).
- **Single source** — generate SDKs and mocks from the spec so they can't disagree; never hand-maintain a second copy of the contract.

## Anti-patterns

- Documenting only happy-path 200s — clients need the 4xx/5xx bodies and codes to handle failure.
- Prose clients must parse (switching on `message` text). Give stable machine-readable `code`/`type`.
- `example` vs `examples` mixups (schema=array, media-type=map) — renders empty or errors in tooling.
- Reusing/renaming `operationId` — silently breaks generated SDKs.
- Fake data (`foo`/`bar`) and fragment-only snippets — show complete, copy-pasteable, realistic requests.
- Screenshots of JSON instead of copyable code blocks.

## Checklists

**Spec quality — before publish:**

- [ ] `openapi: 3.1.x`; nullable via `type: [..., "null"]` (no `nullable:`)
- [ ] Every operation has a unique, stable `operationId` and is tagged
- [ ] All response codes documented incl. every 4xx/5xx, each referencing the shared `Problem` (RFC 9457) schema
- [ ] Rate-limit responses document `X-RateLimit-*` + `Retry-After`; pagination shape documented consistently
- [ ] Security schemes defined and applied (global `security` + per-op overrides); OAuth2 flows/scopes listed
- [ ] `example`/`examples` used correctly per object type; every example validates against its schema
- [ ] Schemas DRY via `$ref` from `components`

**Sync & release — in CI:**

- [ ] Spectral/Redocly lint passes
- [ ] Contract test (Dredd/Schemathesis) runs against the real API and passes
- [ ] `oasdiff breaking` shows no unintended breaking changes (or version bumped + migration guide written)
- [ ] SDKs/mocks regenerated from the spec; `Deprecation`/`Sunset` headers set for retiring versions
