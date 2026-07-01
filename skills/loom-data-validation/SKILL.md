---
name: loom-data-validation
description: Data validation patterns covering schema validation, input sanitization, output encoding, and type coercion. Use for form/API validation with Zod/Pydantic/Joi/JSON Schema, XSS and injection prevention, constraint checks, data pipeline and ML feature validation.
triggers:
  - validate
  - validation
  - schema
  - form validation
  - API validation
  - JSON Schema
  - Zod
  - Pydantic
  - Joi
  - Yup
  - Ajv
  - class-validator
  - sanitize
  - sanitization
  - XSS prevention
  - injection prevention
  - escape
  - encode
  - output encoding
  - whitelist
  - allowlist
  - blacklist
  - denylist
  - coercion
  - ReDoS
  - mass assignment
  - constraint checking
  - invariant validation
  - data pipeline validation
  - ML feature validation
  - custom validators
  - Great Expectations
  - data quality
  - data drift
---

# Data Validation

## Overview

Validate untrusted data at trust boundaries before it flows into your system. This skill covers schema libraries (Zod/Pydantic/Joi/JSON Schema), coercion pitfalls, context-dependent output encoding, injection/XSS/DoS defenses, and pipeline/ML feature validation.

## Core principles (read first)

- **Parse, don't validate.** A validator that returns `bool` throws away work — the caller re-parses or trusts blindly. Return a *typed value* (`Result<User>`, `User | errors`) so downstream code cannot receive unvalidated data. Schema libraries (Zod `.parse`, Pydantic `.model_validate`) do this by construction.
- **Validate at the boundary, once, then trust the typed value inward.** Boundaries: HTTP handlers, queue consumers, file/CLI parsers, pipeline ingestion, cross-service calls.
- **Server-side is authoritative; client-side validation is UX only.** Never rely on it for security — attackers bypass the client entirely.
- **Allowlist > denylist.** Enumerate what's permitted (`enum`, char classes, known hosts). Denylists (blocking `<script>`, `../`, `'`) are always incomplete — encodings, Unicode, and case defeat them.
- **Canonicalize before validating.** Normalize Unicode (NFC), lowercase host, resolve `.`/`..` in paths, decode percent-encoding — *then* check. Validating raw input lets `%2e%2e%2f` or `．` (fullwidth) slip past.
- **Encoding ≠ validation.** Validation decides *accept/reject*; encoding makes a value *safe for a specific sink* (HTML vs attribute vs JS vs URL vs shell vs SQL). A value can be valid and still need encoding at every sink.
- **Limits are validation.** Cap length, array size, object depth, and total payload bytes to stop DoS (JSON bombs, deeply nested payloads, ReDoS amplification).

## Zod (TypeScript)

`safeParse` returns a discriminated result (no throw); `parse` throws `ZodError`. Prefer `safeParse` at boundaries.

```typescript
import { z } from "zod";

const CreateUser = z
  .object({
    email: z.string().trim().toLowerCase().email().max(255),
    password: z.string().min(12).max(128)
      .regex(/[a-z]/).regex(/[A-Z]/).regex(/\d/).regex(/[^A-Za-z0-9]/),
    role: z.enum(["user", "admin", "moderator"]).default("user"),
    tags: z.array(z.string().max(50)).max(10).default([]),
    age: z.number().int().min(13).max(150).optional(),
  })
  .strict();            // reject unknown keys → blocks mass-assignment/overposting

type CreateUserIn = z.input<typeof CreateUser>;   // pre-transform
type CreateUserOut = z.output<typeof CreateUser>; // post-transform (use this downstream)

const parsed = CreateUser.safeParse(req.body);
if (!parsed.success) {
  const errors = parsed.error.issues.map((e) => ({ field: e.path.join("."), message: e.message }));
  return res.status(422).json({ errors });
}
const user = parsed.data; // fully typed + validated
```

- **`.strict()` vs `.strip()` (default) vs `.passthrough()`:** default silently *drops* unknown keys — safe but hides typos. `.strict()` rejects them (best for request bodies). Never `.passthrough()` untrusted input into a DB writer (overposting).
- **`z.discriminatedUnion`** for tagged variants — faster and clearer errors than `z.union`. **`z.lazy`** for recursive schemas.
- **Async rules** (uniqueness, DB lookups) need `.parseAsync`/`.safeParseAsync` — a sync `.parse` on a schema with `.refine(async …)` throws at runtime.
- Zod validates but does **not** sanitize HTML — a valid string can still be an XSS payload. Encode at the sink.

⚠ **Zod coercion footguns** (`z.coerce.*` wraps the JS global constructor):

- `z.coerce.number()` → `Number(x)`: `Number("")===0`, `Number(" ")===0`, `Number(null)===0`, `Number([])===0`. An empty form field becomes `0`, not an error. Prefer `z.string().regex(/^\d+$/).transform(Number)` or guard emptiness first.
- `z.coerce.boolean()` → `Boolean(x)`: **any non-empty string is `true`, including `"false"` and `"0"`.** Use `z.enum(["true","false"]).transform(v => v === "true")` (or `z.stringbool()` in Zod 4).
- HTML forms send `""`, never `null`/`undefined`. `.optional()` (accepts `undefined`), `.nullable()` (accepts `null`), and `.default()` are distinct — an empty text input is `""`, so add `.transform(v => v || undefined)` or `z.literal("").or(realSchema)`.

## Pydantic v2 (Python)

⚠ **v1→v2 migration** — v1 idioms silently break or emit deprecation warnings:

| v1 | v2 |
| --- | --- |
| `@validator('f')` | `@field_validator('f')` + `@classmethod` |
| `@root_validator` | `@model_validator(mode="before"\|"after")` |
| `class Config:` | `model_config = ConfigDict(...)` |
| `constr(regex=...)` | `constr(pattern=...)` / `Annotated[str, StringConstraints(pattern=...)]` |
| `anystr_strip_whitespace` | `str_strip_whitespace` |
| list `max_items` | `max_length` |
| `.dict()` / `.parse_obj()` | `.model_dump()` / `.model_validate()` |

```python
from typing import Annotated, Literal, Optional
from pydantic import BaseModel, ConfigDict, EmailStr, Field, StringConstraints, field_validator

Password = Annotated[str, StringConstraints(min_length=12, max_length=128)]

class CreateUser(BaseModel):
    model_config = ConfigDict(str_strip_whitespace=True, extra="forbid")  # extra="forbid" blocks overposting

    email: EmailStr
    password: Password
    role: Literal["user", "admin", "moderator"] = "user"
    age: Optional[Annotated[int, Field(ge=13, le=150)]] = None
    tags: Annotated[list[str], Field(max_length=10)] = []

    @field_validator("email")
    @classmethod
    def lower(cls, v: str) -> str:
        return v.lower()
```

⚠ **v2 coercion (lax mode, the default):** numeric strings coerce (`"123" → 123`), and `float → int` succeeds only with no fractional part. To reject cross-type coercion use `ConfigDict(strict=True)` or per-field `Field(strict=True)`. `bool` in lax mode accepts `"true"/"yes"/"on"/1` etc. — surprising for API inputs; use strict or `Literal`.

- `extra="forbid"` on request models — default `extra="ignore"` silently drops unknown keys (typos pass, attacker fields ignored but not flagged).
- `EmailStr` needs `email-validator` installed; `ValidationError.errors()` gives structured `loc`/`msg`/`type` for 422 responses.
- `mode="before"` validators run on raw input (coerce/normalize); `mode="after"` run on the typed value (cross-field checks).

## JSON Schema / Ajv (language-agnostic)

Use when the contract must be shared across languages or stored as data (OpenAPI, config schemas).

```typescript
import Ajv from "ajv";
import addFormats from "ajv-formats";
const ajv = new Ajv({ allErrors: true, removeAdditional: true, coerceTypes: true, useDefaults: true });
addFormats(ajv);
const validate = ajv.compile(schema); // compile ONCE at startup, reuse; compiling per-request is a major perf cliff
if (!validate(data)) console.log(validate.errors); // {instancePath, keyword, params, message}
```

⚠ Gotchas:

- **`additionalProperties: false` is not inherited** and does not apply across `allOf`/`anyOf`/`$ref` composition — unknown keys can slip through combined schemas. Set it explicitly on each object.
- `coerceTypes: true` mutates input (`"5" → 5`, `"" → 0` for numbers) — same empty-string trap as JS.
- `removeAdditional`/`useDefaults` **mutate** the validated object in place; clone first if you need the original.
- `format` (email, uri, date-time) requires `ajv-formats`; unknown formats are ignored silently unless `strict: true`.

## Type coercion pitfalls (cross-cutting)

| Input | JS `Number()` | Note |
| --- | --- | --- |
| `""`, `" "`, `null`, `[]` | `0` | empty field silently becomes zero |
| `"123abc"`, `undefined`, `{}` | `NaN` | `NaN` passes `typeof === "number"` |
| `"0x1F"`, `"1e3"` | `31`, `1000` | hex/exponent accepted |

- `parseInt("123px") → 123` (lenient), `parseInt("") → NaN`. `parseFloat` similar. Prefer strict regex + explicit conversion for untrusted numerics.
- Ints beyond `Number.MAX_SAFE_INTEGER` (2^53) lose precision — validate large IDs/money as strings (`BigInt`, decimal).
- Empty string vs null vs absent: three distinct states. Decide per field which are allowed; don't let coercion collapse them.

## Sanitization and output encoding

**Encode at the sink, for the sink's context. One escaper does not fit all contexts.**

```typescript
import DOMPurify from "dompurify";      // browser: window; server: DOMPurify(new JSDOM("").window)
// Rendering user HTML (rich text): sanitize with an ALLOWLIST of tags/attrs
const clean = DOMPurify.sanitize(dirty, {
  ALLOWED_TAGS: ["b", "i", "em", "strong", "a", "p", "ul", "ol", "li"],
  ALLOWED_ATTR: ["href"],
  ALLOW_DATA_ATTR: false,
});
```

Context-dependent encoding (choose by *where the value lands*):

| Sink | Escape | Why hand-rolling fails |
| --- | --- | --- |
| HTML text | `&<>"'` → entities | — |
| HTML attribute | entity-encode + **always quote** | unquoted attr breaks on space/`/` |
| `<script>` / JS string | `\xHH` for `< > & / ' "` | `</script>` in a JS string ends the tag |
| URL component | `encodeURIComponent` | but `javascript:` scheme still executes — allowlist scheme |
| CSS value | `\HH` hex (space-terminated) | `expression()`/`url()` injection |
| SQL | **parameterized queries** | escaping is not a substitute |
| Shell | avoid; pass argv arrays, never a string | quoting is unwinnable |

⚠ Prefer framework auto-escaping (React JSX, Jinja2 autoescape, template engines) over manual encoders. Manual encoding is for the gaps (`dangerouslySetInnerHTML`, `|safe`, building HTML strings). React escapes text but **not** `href="javascript:…"`, `dangerouslySetInnerHTML`, or `<script>` content.

**Never build SQL/HTML/shell by string concatenation.** Parameterized queries for SQL; argv arrays for shells; DOM APIs / templating for HTML.

**Path traversal:** canonicalize then confine.

```typescript
import path from "node:path";
const resolved = path.resolve(baseDir, userPath);
if (resolved !== baseDir && !resolved.startsWith(baseDir + path.sep)) throw new Error("traversal");
```

⚠ The naive `resolved.startsWith(baseDir)` check has a prefix bug: `/srv/data-evil` starts with `/srv/data`. Append the separator (as above). Decode percent-encoding before resolving.

## Security-focused validation

- **XSS:** output-encode at every sink (above); sanitize stored HTML with an allowlist; set CSP as defense-in-depth. Validation alone does not stop XSS.
- **Injection (SQL/NoSQL/LDAP/command):** parameterize / bind; never interpolate. For NoSQL (Mongo), reject object-typed values where a scalar is expected (`{"$gt": ""}` operator injection).
- **Mass assignment / overposting:** `.strict()` (Zod) / `extra="forbid"` (Pydantic) / explicit field allowlists. Never bind a request body straight onto an ORM model — an attacker sets `isAdmin`/`role`.
- **ReDoS:** user-supplied *or* poorly written regexes with nested/overlapping quantifiers (`(a+)+`, `(.*)*`, `(a|a)*`) backtrack super-linearly → CPU DoS. Mitigate: avoid nested quantifiers, anchor patterns, cap input length before matching, and for untrusted patterns use a linear engine (Google RE2 / `re2` bindings) or a match timeout. Classic offender: catastrophic email/whitespace regexes.
- **DoS via size/depth:** enforce max body bytes (server + framework), max array length, max object nesting depth, and max string length *before* deep validation. JSON parsers don't bound depth by default → "billion laughs"-style expansion and stack exhaustion.
- **Canonicalization attacks:** Unicode NFC/NFKC normalize, casefold, decode, and resolve *before* allowlist checks (see Core principles).
- **File uploads:** validate by *content* (magic bytes / sniff), not just extension or client `Content-Type` (both attacker-controlled); cap size; store outside webroot; generate server-side filenames.

## Data pipeline validation

```python
import great_expectations as gx
validator = gx.from_pandas(df)
validator.expect_column_values_to_not_be_null("email")
validator.expect_column_values_to_be_unique("email")
validator.expect_column_values_to_be_in_set("status", ["active", "inactive", "pending"])
validator.expect_column_values_to_be_between("age", 0, 150)
result = validator.validate()
if not result.success:
    raise ValueError([r for r in result.results if not r.success])
```

- Validate **at ingestion** and fail loudly; a silently-wrong pipeline corrupts every downstream table.
- Add freshness/recency checks (dbt `dbt_utils.recency`) — stale-but-valid data is a common silent failure.
- Track row-count deltas and null-rate deltas between runs, not just absolute thresholds.

## ML feature validation (train/serve consistency)

Guard against the drift that silently degrades models:

- **Schema parity:** serving features must match training columns, dtypes, and order. A renamed/missing column often defaults to null → garbage predictions with no error.
- **Distribution drift:** compare serving vs training via PSI or KS test per feature; alert on shift.
- **Categorical drift:** reject/flag unseen categories (encoders map them to 0/UNK, skewing output).
- **Null-rate spikes** and **range violations** (values outside training min/max) — clamp or reject per policy.
- **train/serve skew:** the same transformation code must run in both paths; fit scalers/encoders on training data only and persist them (fitting at serve time is leakage + skew).

## Checklists

Boundary validation — verify before done:

- [ ] Every external input (HTTP body/query/params, headers, queue msg, file, CLI arg, cross-service payload) is parsed into a typed value at the boundary
- [ ] Validator returns typed data, not a bool (parse-don't-validate)
- [ ] Unknown keys rejected on request bodies (`.strict()` / `extra="forbid"`) — no overposting
- [ ] Length/size/array-length/depth limits set; max request-body bytes enforced at the server
- [ ] Coercion audited: empty-string→0, `"false"`→true, large-int precision handled
- [ ] Allowlists (enums, char classes, known hosts/schemes), not denylists
- [ ] Canonicalize (Unicode NFC, path normalize, decode) *before* validating
- [ ] Server-side validation present even where the client already validates

Output/security — verify before done:

- [ ] Output encoded per sink (HTML/attr/JS/URL/CSS); framework auto-escaping on, manual only in gaps
- [ ] SQL parameterized; shells use argv arrays; no string-concatenated queries/commands
- [ ] Stored HTML sanitized with an allowlist (DOMPurify); CSP set
- [ ] No user-controlled or catastrophically-backtracking regex on unbounded input (ReDoS)
- [ ] File uploads validated by content, size-capped, stored outside webroot with server-generated names
- [ ] Error messages are actionable but leak no internals (stack traces, SQL, paths) in production
