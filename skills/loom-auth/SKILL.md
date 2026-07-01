---
name: loom-auth
description: Authentication and authorization patterns including OAuth2, JWT, RBAC/ABAC, session management, API keys, password hashing, and MFA. Use for login flows, access control, identity management, tokens, permissions, and API key authentication. Do not use for vulnerability scanning (use loom-security-scan), audits (loom-security-audit), or threat modeling (loom-threat-model).
triggers:
  - login
  - logout
  - signin
  - signup
  - register
  - authentication
  - authorization
  - password
  - credential
  - token
  - JWT
  - OAuth
  - OAuth2
  - OIDC
  - OpenID
  - SSO
  - SAML
  - session
  - cookie
  - refresh token
  - access token
  - bearer
  - authorization header
  - auth header
  - 401
  - 403
  - forbidden
  - unauthorized
  - RBAC
  - ABAC
  - permissions
  - roles
  - access control
  - IDOR
  - BOLA
  - object-level authorization
  - identity
  - MFA
  - 2FA
  - two-factor
  - multi-factor
  - TOTP
  - WebAuthn
  - passkey
  - API key
  - auth flow
  - PKCE
  - client credentials
  - timing-safe
  - argon2
---

# Authentication & Authorization

Authentication proves *who* you are; authorization decides *what* you may touch. **Authorization is where real bugs cluster** — broken object-level authorization (IDOR/BOLA) is the #1 API risk (OWASP API Security Top 10). Get object-ownership checks right before polishing token plumbing.

## When to Use

Auth is security-critical; default to Opus (`loom-senior-software-engineer`) for design, token/session lifecycle, access-control model choice, and anything touching production credentials. Delegate to Sonnet only for well-scoped execution against an existing pattern: unit tests for auth code, boilerplate middleware, scaffolding from a concrete plan. Never ship auth code that a senior hasn't adversarially reviewed against the checklist below.

Scope boundary: this skill is *mechanisms*. For vuln scanning use `loom-security-scan`; deep audit `loom-security-audit`; architecture threats `loom-threat-model`.

## Authorization: IDOR / BOLA (read this first)

Every request that names an object (`/orders/123`, `?user_id=…`, a foreign key in a body) must verify the caller **owns or is granted** that specific object — authentication ("is logged in") is not authorization. The bug is invisible in tests that only use one account.

```typescript
// ❌ BOLA: any authenticated user reads any order by guessing the id
app.get("/orders/:id", requireAuth, async (req, res) => {
  const order = await db.orders.findUnique({ where: { id: req.params.id } });
  res.json(order);
});

// ✅ Scope the query by the authenticated principal (and tenant)
app.get("/orders/:id", requireAuth, async (req, res) => {
  const order = await db.orders.findFirst({
    where: { id: req.params.id, userId: req.user.id }, // ownership in the WHERE
  });
  if (!order) return res.status(404).end(); // 404 not 403: don't confirm existence
  res.json(order);
});
```

Rules: enforce ownership/tenant in the **query filter**, not a post-fetch `if`; check on writes and nested/batch operations too; never trust an `id`/`role`/`tenant` from the client body (mass-assignment → privilege escalation); default-deny. In multi-tenant systems scope every query by `tenant_id` — a missing tenant filter is cross-tenant data leakage.

## OAuth2 Flows

| Flow | Use for | Notes |
| ---- | ------- | ----- |
| Authorization Code + PKCE | Server apps AND SPAs/mobile | PKCE now recommended for **all** clients, not just public ones |
| Client Credentials | Service-to-service | No user context; scope tightly |
| Device Code | TVs/CLI with no browser | — |
| ~~Implicit~~, ~~Password (ROPC)~~ | — | Deprecated by OAuth 2.1; do not use |

```typescript
// Authorization Code + PKCE
authUrl.searchParams.set("response_type", "code");
authUrl.searchParams.set("client_id", CLIENT_ID);
authUrl.searchParams.set("redirect_uri", REDIRECT_URI); // MUST match a registered exact URI
authUrl.searchParams.set("scope", "openid profile email");
authUrl.searchParams.set("state", generateSecureState());     // CSRF: verify on callback
authUrl.searchParams.set("code_challenge", challenge);        // SHA-256(verifier), base64url
authUrl.searchParams.set("code_challenge_method", "S256");    // never "plain"
// token exchange (server-side) includes: grant_type=authorization_code, code, code_verifier
```

⚠ Gotchas: validate `state` on the callback (CSRF); validate `redirect_uri` against an exact allowlist (open-redirect / code interception); `code_challenge_method=S256` only; keep `client_secret` server-side; for OIDC validate the ID token's `nonce`.

## JWT

```typescript
function signToken(payload) {
  return jwt.sign({ ...payload, iat: now() }, PRIVATE_KEY, {
    algorithm: "RS256", expiresIn: "15m",
    issuer: "https://api.example.com", audience: "https://app.example.com",
  });
}
function verifyToken(token) {
  return jwt.verify(token, PUBLIC_KEY, {
    algorithms: ["RS256"],            // PIN the algorithm — never read alg from the token
    issuer: "https://api.example.com",
    audience: "https://app.example.com",
  });
}
```

### JWT footguns (each is a real CVE class)

- **`alg: none`** — a token with no signature. Mitigation: pass an explicit `algorithms` allowlist to verify; never let the library pick from the header.
- **RS256 → HS256 key confusion** — attacker flips `alg` to HS256 and signs with your *public* key (which HS256 treats as the HMAC secret). Mitigation: pin `algorithms: ["RS256"]`; never accept the token's declared alg.
- **Missing claim validation** — always verify `exp`, `nbf`, `iss`, and `aud`. A valid signature only proves *who issued it*, not *for whom* — without `aud` a token minted for service A is replayable at service B.
- **No revocation** — JWTs are valid until `exp`. For logout/ban, keep access tokens short (≤15 min) and gate refresh through a server-side allowlist (below), or maintain a denylist of `jti`.
- **Sensitive data in payload** — JWT is signed, not encrypted; base64 is readable. No PII/secrets in claims.
- **HS256 in distributed systems** — every verifier needs the shared secret, so any one service can mint tokens. Prefer RS256/ES256 (asymmetric) so verifiers hold only the public key.

### Refresh token rotation (enables revocation)

```typescript
async function refreshTokens(refreshToken) {
  const stored = await db.refreshTokens.findUnique({ where: { token: hashToken(refreshToken) } });
  if (!stored || stored.expiresAt < new Date()) throw new UnauthorizedError();
  // Reuse detection: if a rotated (deleted) token is presented again, treat as theft →
  // revoke the whole token family for that user.
  await db.refreshTokens.delete({ where: { id: stored.id } }); // one-time use
  const next = generateSecureToken();
  await db.refreshTokens.create({ data: { token: hashToken(next), userId: stored.userId, expiresAt: addDays(new Date(), 7) } });
  return { accessToken: signToken({ sub: stored.userId }), refreshToken: next };
}
```

Store refresh tokens **hashed** (they are password-equivalent); short access (≤15 min), refresh ≤7 days; deliver refresh tokens in `httpOnly; Secure; SameSite` cookies, never in JS-readable storage.

## RBAC / ABAC

RBAC = permissions grouped into roles (simple, coarse). ABAC = policy functions over subject/resource/action/environment attributes (fine-grained, dynamic — needed for ownership, department, time-of-day, tenant).

```typescript
// RBAC middleware — expand roles to permissions, honor ":own" resource scope
function requirePermission(permission) {
  return (req, res, next) => {
    const perms = req.user.roles.flatMap((r) => ROLES[r]);
    const [resource, action, scope] = permission.split(":");
    const ok = perms.includes(permission) ||
      (scope === "own" && perms.includes(`${resource}:${action}`));
    if (!ok) return res.status(403).json({ error: "Forbidden" });
    next(); // NOTE: ":own" still requires a per-object ownership check (see IDOR/BOLA)
  };
}
```

```typescript
// ABAC — every policy must pass (default-deny)
const policies = [
  (ctx) => ctx.resource.owner === ctx.subject.id,
  (ctx) => ctx.subject.roles.includes("manager") && ctx.resource.department === ctx.subject.department,
];
const allow = (ctx) => policies.every((p) => p(ctx));
```

⚠ Middleware-level RBAC checks the *endpoint*; it does not prove the caller owns the *specific object* the route resolves to. Function-level auth without object-level auth is still BOLA.

## Sessions

```typescript
async function createSession(userId, req) {
  const sessionId = crypto.randomUUID();          // ≥128 bits entropy from a CSPRNG
  await redis.setex(`session:${sessionId}`, SESSION_TTL,
    JSON.stringify({ userId, createdAt: Date.now(), ip: req.ip, ua: req.headers["user-agent"] }));
  await redis.sadd(`user-sessions:${userId}`, sessionId); // index for bulk invalidation
  return sessionId;
}
```

Cookie flags: `httpOnly; Secure; SameSite=Lax|Strict; Path=/`. **Regenerate the session ID on privilege change (login, step-up)** to kill session fixation. Absolute (24h) + idle (30m) timeouts. Clear ALL sessions on password change/reset. Store session IDs server-side hashed if they double as bearer tokens.

## API Keys

```typescript
function generateApiKey() {
  const key = `sk_live_${crypto.randomBytes(32).toString("base64url")}`;
  return { key, prefix: key.slice(0, 16), hash: sha256(key) }; // show key ONCE; store only the hash
}
```

Store only the **hash** (fast SHA-256 is fine here — keys are high-entropy, unlike passwords). Prefix (`sk_live_…`) enables identification, log redaction, and secret-scanner detection. Attach scopes + per-key rate limit + expiry. Rotate without downtime by allowing N active keys per principal.

## Password Hashing

```typescript
// argon2id — DEFAULT for new systems (memory-hard → resists GPU/ASIC cracking)
await argon2.hash(password, {
  type: argon2.argon2id,
  memoryCost: 65536,  // 64 MiB (OWASP floor 19 MiB; raise until ~0.5–1s/hash on your hardware)
  timeCost: 3,        // iterations
  parallelism: 4,
});
// bcrypt — acceptable legacy; cost ≥ 12
await bcrypt.hash(password, 12);
```

⚠ Why these and not others:

- **argon2id** blends argon2i (side-channel resistance) + argon2d (GPU resistance) — the recommended default. Tune params to your box, not blindly copy.
- **bcrypt silently truncates input at 72 bytes** — long passwords / a pepper appended past 72 bytes are ignored. Pre-hash (`base64(sha256(pw))`) before bcrypt if you must exceed 72 bytes.
- **Never** MD5/SHA-1/SHA-256/unsalted for passwords — GPUs do billions/sec; fast hashes are the wrong tool. Salt is built into argon2/bcrypt output; don't roll your own.
- Verify with the library's `verify`/`compare` (constant-time). Never `hash(input) == stored`.
- Peppers (an app-side secret added before hashing) belong in a KMS/HSM, not the DB.

## MFA

```typescript
// TOTP: verify with a ±1 step window for clock skew; store the secret encrypted at rest
authenticator.verify({ token, secret }); // otplib default window handles skew
```

Prefer **WebAuthn/passkeys** (phishing-resistant, bound to origin) > TOTP > SMS (SIM-swap/interceptable — backup only). Issue 10 single-use backup codes, stored hashed. Require MFA **re-verification** (step-up) for sensitive actions (password/email change, payouts). Rate-limit TOTP attempts (brute-force is 10⁶ codes).

## Timing-Safe Comparison

Comparing secrets with `==`/`===`/`strcmp` leaks length and prefix via early-exit timing — an attacker measures response time to recover a token/HMAC byte-by-byte. Use constant-time compare for **any** secret equality: session tokens, API keys, password-reset tokens, HMAC/webhook signatures, MFA codes.

```typescript
import { timingSafeEqual } from "crypto";
function safeEqual(a: string, b: string): boolean {
  const ba = Buffer.from(a), bb = Buffer.from(b);
  if (ba.length !== bb.length) return false; // length is not secret here; lengths already differ
  return timingSafeEqual(ba, bb);
}
```

Equivalents: Python `hmac.compare_digest`, Go `crypto/subtle.ConstantTimeCompare`, Rust `subtle`/`ring`. For hashed values (argon2/bcrypt) the library's verify already handles this.

## Security Checklist (verify before done)

Credentials & tokens:

- [ ] Passwords hashed with argon2id (or bcrypt cost ≥12); no fast/unsalted hashes; input ≤72 bytes for bcrypt or pre-hashed
- [ ] Refresh tokens, API keys, MFA secrets, reset tokens stored **hashed/encrypted**, never plaintext
- [ ] JWT verify pins `algorithms`; validates `exp`, `nbf`, `iss`, `aud`; RS256/ES256 (no `alg:none`, no HS256 in distributed systems)
- [ ] No secrets in JWT claims, logs, URLs, or error messages
- [ ] All secret/token/HMAC comparisons are constant-time

Access control:

- [ ] Every object access checks ownership/tenant in the query filter (no IDOR/BOLA); default-deny
- [ ] Write/batch/nested operations authorized, not just reads
- [ ] Client cannot set `id`/`role`/`tenant`/`isAdmin` via mass assignment
- [ ] `:own`-scoped permissions still perform per-object checks

Flows & sessions:

- [ ] OAuth: `state` verified, `redirect_uri` allowlisted (exact), PKCE `S256`
- [ ] Session ID regenerated on login/step-up; `httpOnly; Secure; SameSite` cookies; absolute + idle timeouts; all sessions cleared on password change
- [ ] Rate limiting + lockout on auth endpoints; identical error for bad-user vs bad-password (no user enumeration); timing between the two paths comparable
- [ ] MFA re-verified for sensitive actions; backup codes hashed & single-use
- [ ] HTTPS/HSTS enforced on all auth endpoints; auth events logged for audit (without secrets)

## Complete Login Flow (reference)

```typescript
async function login(email, password, mfaCode) {
  const user = await db.users.findUnique({ where: { email } });
  // Run a dummy verify even when user is null so timing doesn't reveal account existence.
  const ok = await verifyPasswordArgon2(user?.passwordHash ?? DUMMY_HASH, password);
  if (!user || !ok) {
    if (user) await incrementFailedAttempts(user.id);
    throw new UnauthorizedError("Invalid credentials"); // same message either branch
  }
  if (user.lockedUntil && user.lockedUntil > new Date()) throw new UnauthorizedError("Account locked");
  if (user.mfaEnabled) {
    if (!mfaCode) return { requiresMfa: true, mfaToken: generateMfaToken(user.id) };
    if (!(await verifyMFA(user.id, mfaCode))) throw new UnauthorizedError("Invalid MFA code");
  }
  await db.users.update({ where: { id: user.id }, data: { failedAttempts: 0, lockedUntil: null } });
  return { accessToken: signToken({ sub: user.id, roles: user.roles }), refreshToken: await createRefreshToken(user.id) };
}
```
