---
name: loom-api-design
description: Designs RESTful APIs, GraphQL schemas, and RPC interfaces for consistency, usability, and scalability. Use when defining endpoints, resource models, HTTP semantics, pagination, versioning, or RPC service contracts.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
triggers:
  - api design
  - REST
  - RESTful
  - REST API
  - GraphQL
  - GraphQL schema
  - gRPC
  - OpenAPI
  - Swagger
  - endpoint
  - route
  - resource
  - CRUD
  - HTTP method
  - GET
  - POST
  - PUT
  - PATCH
  - DELETE
  - status code
  - pagination
  - filtering
  - sorting
  - versioning
  - HATEOAS
  - API versioning
  - schema design
  - RPC
  - service design
  - idempotency key
  - problem+json
  - RFC 9457
---

# API Design

## Overview

Design intuitive, consistent, evolvable API contracts across REST, GraphQL, and gRPC. Get the resource model, HTTP semantics, error envelope, pagination, and versioning right up front — they are the hardest things to change once clients depend on them.

## Design Workflow

1. Model resources and relationships before URLs; name the nouns, then map operations onto them.
2. Pick the paradigm: REST (resource CRUD, caching, broad tooling), GraphQL (client-shaped reads, aggregation, mobile), gRPC (internal, low-latency, streaming, strong contracts).
3. Lock the **error envelope**, **pagination shape**, and **versioning policy** once — reuse everywhere.
4. Design for change: additive evolution, tolerant readers, deprecation windows.
5. Write the spec (OpenAPI/SDL/proto) as the source of truth; generate clients/servers from it.

## REST

### Resource & URL conventions

- Nouns, plural collections: `/users`, `/users/{id}`, `/users/{id}/orders`. Never verbs in paths (`/getUsers` is wrong — the method is the verb).
- Pick one case (`snake_case` or `camelCase`) and keep it identical across paths, query params, and bodies.
- Keep nesting shallow (≤2 levels); deep hierarchies couple resources. Prefer `/orders?user_id=` over `/users/{id}/orders/{oid}/items/...`.
- Sub-resource actions that aren't CRUD: model as a resource (`POST /orders/{id}/refunds`) rather than an RPC verb (`POST /orders/{id}/refund`) when the action produces a trackable entity.

### HTTP methods, safety, idempotency

| Method | Purpose | Safe | Idempotent | Body |
| ------ | ------- | ---- | ---------- | ---- |
| GET | Read | yes | yes | no |
| HEAD | Read headers | yes | yes | no |
| POST | Create / non-idempotent action | no | **no** | yes |
| PUT | Full replace (client sets full state) | no | yes | yes |
| PATCH | Partial update | no | not inherently | yes |
| DELETE | Remove | no | yes | maybe |

- **Idempotent** = same request repeated yields same server state (not same response — a repeated DELETE may return 404 the second time; that's fine).
- PUT replaces the entire resource — omitted fields are cleared. If a client can't send full state, use PATCH.

### Status codes — get the ambiguous ones right

| Code | Use for | Not for |
| ---- | ------- | ------- |
| 200 | OK with body | creates (use 201) |
| 201 | Created; return `Location` + body | |
| 202 | Accepted, async processing pending | sync completion |
| 204 | Success, no body (DELETE, some PUT) | |
| 400 | Malformed **syntax** (bad JSON, wrong type) | valid-but-rejected semantics |
| 401 | Missing/invalid **authentication** | authorization failures |
| 403 | Authenticated but not **authorized** | missing auth (401) |
| 404 | Not found (or hide existence from 403) | |
| 409 | **State conflict**: duplicate, optimistic-concurrency/version clash | field validation |
| 422 | Well-formed but **semantically invalid** (business-rule / field validation) | syntax errors (400) |
| 428 | Precondition Required — force `If-Match` to prevent lost updates | |
| 412 | Precondition Failed — `If-Match`/ETag mismatch | |
| 429 | Rate limited; include `Retry-After` | |
| 503 | Temporarily down; include `Retry-After` | |

⚠ 400 vs 422: 400 = "I can't parse this." 422 = "I parsed it, but `age: -5` violates a rule." Pick one convention for field-validation errors (422 is the modern default) and apply it everywhere. ⚠ 409 vs 422: 409 is about **resource state** (email already taken, stale version); 422 is about the **payload's** semantics.

### PATCH: Merge Patch vs JSON Patch

Two incompatible standards — declare which via `Content-Type`:

- **JSON Merge Patch** (RFC 7386, `application/merge-patch+json`): send a partial object; present keys overwrite, `null` **deletes** the member. Simple, but you **cannot set a value to null** and **cannot edit array elements** (arrays are replaced wholesale).
- **JSON Patch** (RFC 6902, `application/json-patch+json`): an ordered array of ops (`add`/`remove`/`replace`/`move`/`copy`/`test`) with JSON Pointer paths. Handles arrays, nulls, and conditional updates; verbose. `test` enables optimistic concurrency inside the patch.

```json
// merge-patch: clears "nickname", sets name
{ "name": "Ada", "nickname": null }
// json-patch: equivalent + array edit
[ { "op": "replace", "path": "/name", "value": "Ada" },
  { "op": "remove", "path": "/nickname" },
  { "op": "add", "path": "/tags/-", "value": "vip" } ]
```

### Idempotency keys (safe POST retries)

For non-idempotent creates/payments, accept a client-generated `Idempotency-Key` header. Store `key → (request fingerprint, response)` for a window (e.g., 24h). On replay: same key + same body ⇒ return the stored response; same key + **different** body ⇒ 422/409 (key reuse). Scope keys per endpoint + authenticated principal. This is the correct fix for "user double-clicked / client retried on timeout."

### Pagination

| Style | Pros | Cons / gotchas |
| ----- | ---- | -------------- |
| Offset/limit (`?page=&per_page=`) | random access, total counts, trivial | **drifts under concurrent writes** (inserts/deletes shift the window → skipped or duplicated rows); deep offsets are slow (DB scans+discards N rows) |
| Cursor/keyset (`?after=<opaque>`) | stable under writes, O(1) via indexed `WHERE key > cursor` | no jump-to-page; needs a stable total-order sort key |

⚠ Cursor correctness: the sort key **must be unique and total-ordering**, or rows sharing a value get skipped/duplicated at page boundaries. Tie-break on a unique column: `ORDER BY created_at, id` and encode both in the cursor. ⚠ Make cursors **opaque** (base64 the keyset) so clients can't depend on internals and you can evolve the scheme. Default and cap `limit` (e.g., default 20, max 100) to protect the backend. Prefer cursor for infinite scroll / large or write-heavy datasets; offset only for small, admin-style, jump-to-page needs.

### Filtering, sorting, field selection

- Filter via query params: `?status=active&min_price=10`. Sort with a signed key list: `?sort=-created_at,name` (`-` = desc).
- Sparse fieldsets / expansion to control payload size: `?fields=id,email` and `?expand=customer`.
- Keep filter grammar simple; a full query DSL in query strings is a maintenance trap — reach for GraphQL if clients truly need arbitrary queries.

### Error envelope — standardize on RFC 9457 (problem+json)

RFC 9457 (obsoletes 7807), media type `application/problem+json`. Members: `type` (URI identifying the problem class), `title`, `status`, `detail`, `instance`, plus domain extensions.

```json
{
  "type": "https://api.example.com/problems/validation-error",
  "title": "Request validation failed",
  "status": 422,
  "detail": "One or more fields are invalid.",
  "instance": "/users",
  "errors": [
    { "field": "email", "code": "invalid_format", "message": "Not a valid email" },
    { "field": "age", "code": "out_of_range", "message": "Must be 18–120" }
  ],
  "request_id": "req_abc123"
}
```

Rules: machine-readable stable `type`/`code` (clients switch on these, never on `title`/`message` prose); a `request_id` for support/tracing; a `errors` array for per-field validation. Never leak stack traces, SQL, or internal hostnames.

### Versioning

| Strategy | Pros | Cons |
| -------- | ---- | ---- |
| URI path `/v2/users` | explicit, cache-friendly, easy to route/test | version pinned to URL; coarse-grained |
| Media type `Accept: application/vnd.api.v2+json` | clean URLs, granular | harder to test/curl; caches need `Vary: Accept` |
| Custom header `API-Version: 2` | clean URLs | invisible in logs/URLs; `Vary` caution |
| Query `?version=2` | trivial | pollutes caching/URLs; least recommended |

- **Version only for breaking changes.** Additive changes (new endpoints, new optional fields, new enum values *if clients tolerate unknowns*) need no bump — bake "tolerant reader" into client guidance.
- Breaking = removing/renaming fields, changing types/meaning, changing auth or error format, tightening validation.
- Deprecate with `Deprecation` and `Sunset` response headers (RFC 8594) + `Link rel="successor-version"`; support N-1 for a published window; publish a migration guide.

## GraphQL

- **Don't version** — evolve additively; mark removals with `@deprecated(reason:)`. Avoid nullable-to-non-null tightening (breaking).
- Relay-style connections for lists: `edges { node cursor }`, `pageInfo { hasNextPage endCursor }`. Global `Node` interface + opaque IDs for refetch/caching.
- Separate input types per mutation (`CreateUserInput`, `UpdateUserInput`); mutations return a **payload** carrying both `data` and typed `errors`, not just top-level `errors`.
- Nullability is a contract: a non-null field that errors **nulls its nearest nullable parent** — model expected-failure fields as nullable so one error doesn't blank a whole response.
- ⚠ **N+1 by construction**: nested resolvers fan out into per-row fetches. Batch with DataLoader. ⚠ Untrusted clients can craft deep/expensive queries — enforce **depth and complexity limits** and persisted queries.

## gRPC / Protobuf

- Methods are verbs: `GetUser`, `ListUsers`, `CreateUser`. Package-version namespaces: `myapi.v1`.
- **Field numbers are the wire contract**: never change or reuse a number; `reserved` deleted numbers/names. Adding fields is backward-compatible; changing a field's type or number is not.
- Cursor pagination via `page_size` + `page_token` / `next_page_token` (AIP-158). Rich errors via `google.rpc.Status` + status codes; use canonical codes (`NOT_FOUND`, `ALREADY_EXISTS`, `FAILED_PRECONDITION`, `INVALID_ARGUMENT`).
- Use `optional` (proto3) to distinguish "unset" from zero-value where it matters. Leverage streaming (server/client/bidi) instead of polling.

## Examples

### REST: paginated collection (OpenAPI 3.1)

```yaml
paths:
  /products:
    get:
      summary: List products
      parameters:
        - { name: category, in: query, schema: { type: string } }
        - { name: after, in: query, schema: { type: string }, description: Opaque cursor }
        - { name: limit, in: query, schema: { type: integer, default: 20, maximum: 100 } }
      responses:
        "200":
          description: Cursor-paginated products
          content:
            application/json:
              schema:
                type: object
                properties:
                  data: { type: array, items: { $ref: "#/components/schemas/Product" } }
                  page_info:
                    type: object
                    properties:
                      end_cursor: { type: [string, "null"] }   # 3.1 nullable via type array
                      has_next_page: { type: boolean }
    post:
      summary: Create product
      parameters:
        - { name: Idempotency-Key, in: header, schema: { type: string } }
      responses:
        "201": { description: Created }
        "422": { description: Validation error }
```

### GraphQL: mutation payload + connection

```graphql
type Query {
  users(filter: UserFilter, first: Int = 20, after: String): UserConnection!
}
type Mutation {
  createUser(input: CreateUserInput!): CreateUserPayload!
}
type CreateUserPayload {
  user: User
  errors: [UserError!]!   # typed, in-payload errors
}
type UserConnection {
  edges: [UserEdge!]!
  pageInfo: PageInfo!
}
type UserEdge { node: User!  cursor: String! }
```

### gRPC: list with pagination + reserved fields

```protobuf
syntax = "proto3";
package ecommerce.v1;

service ProductService {
  rpc ListProducts(ListProductsRequest) returns (ListProductsResponse);
}
message Product {
  string id = 1;
  string name = 2;
  int64 price_cents = 4;
  reserved 3;            // never reuse a retired field number
  reserved "legacy_sku";
}
message ListProductsRequest {
  string category = 1;
  int32 page_size = 2;   // caps server-side
  string page_token = 3; // opaque cursor
}
message ListProductsResponse {
  repeated Product products = 1;
  string next_page_token = 2;
}
```

## Checklists

**Contract design — verify before publishing:**

- [ ] Resources are nouns; casing consistent across path/query/body; nesting ≤2 levels
- [ ] Every operation's success **and** error status codes are chosen with 400/401/403/404/409/422/429 used per their semantics
- [ ] One error envelope (problem+json) with stable machine-readable `code`/`type` + `request_id`, reused everywhere
- [ ] Pagination shape fixed (cursor for large/write-heavy); cursors opaque; sort key unique/total-ordering with tie-break; `limit` capped
- [ ] PATCH semantics declared (merge-patch vs json-patch) via `Content-Type`
- [ ] Non-idempotent creates accept `Idempotency-Key`; optimistic concurrency via ETag/`If-Match` where lost-update matters
- [ ] Versioning policy chosen; only breaking changes bump; `Deprecation`/`Sunset` headers + migration guide planned

**Evolution safety — before shipping a change:**

- [ ] Change classified breaking vs additive; breaking ⇒ new version, not silent
- [ ] New fields optional; unknown-field tolerance documented for clients
- [ ] GraphQL: removals `@deprecated`, not deleted; gRPC: field numbers untouched, retired numbers `reserved`
- [ ] No secrets, stack traces, or internal identifiers leak in error bodies
