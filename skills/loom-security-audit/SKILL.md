---
name: loom-security-audit
description: Comprehensive security audits identifying vulnerabilities, misconfigurations, and best-practice violations across applications, APIs, infrastructure, and data pipelines. Use for OWASP Top 10 reviews, compliance assessments (SOC2, PCI-DSS, HIPAA, GDPR), threat modeling, risk assessment, and hardening.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
triggers:
  - security audit
  - vulnerability assessment
  - penetration test
  - pentest
  - OWASP
  - CVE
  - security review
  - risk assessment
  - compliance
  - SOC2
  - PCI-DSS
  - HIPAA
  - GDPR
  - security checklist
  - security hardening
  - attack surface
  - security posture
  - API security
  - cloud security
  - container security
  - Kubernetes security
  - data security
  - ML model security
---

# Security Audit

Deep, methodical security review producing evidenced, severity-ranked, remediable findings — the heavyweight companion to `loom-security-scan` (fast tooling). Delegates: STRIDE/architecture → `loom-threat-model`; authn/authz mechanisms → `loom-auth`; dependency/SBOM/supply-chain → `loom-dependency-scan`.

## Method

1. **Scope** — assets, data classification, compliance obligations, threat model (pull from `loom-threat-model`). Define what "in scope" means before touching anything.
2. **Review by layer** — app code, APIs, infra/IaC, data pipelines, ML (sections below).
3. **Evidence** — every finding cites `file:line` or config path + a concrete exploit scenario. A finding without a repro is a guess.
4. **Rate** — CVSS or Likelihood×Impact; rank most-severe first.
5. **Remediate** — specific fix (ideally a diff), not "sanitize inputs".
6. **Report** — executive summary + technical detail + prioritized remediation.

Run tooling first (`loom-security-scan`) to clear known-pattern noise, then spend human effort on **logic and authorization flaws that scanners miss** — that's where audits earn their keep.

## OWASP Top 10 (2021) — audit lens

| # | Category | First things to check |
| - | -------- | --------------------- |
| A01 | Broken Access Control | IDOR/BOLA, missing function-level authz, path traversal, CORS, force-browsing. **Most common; start here.** → `loom-auth` |
| A02 | Cryptographic Failures | Plaintext/weak-hash secrets, TLS < 1.2, weak ciphers, hardcoded keys, ECB mode |
| A03 | Injection | SQL/NoSQL/command/LDAP/XPath, XSS, ORM raw queries |
| A04 | Insecure Design | Missing threat model, no rate limiting by design, trust assumptions |
| A05 | Security Misconfiguration | Default creds, verbose errors, open cloud storage, missing headers, debug on |
| A06 | Vulnerable Components | Outdated deps with CVEs → `loom-dependency-scan` |
| A07 | Identification & Auth Failures | Weak passwords, no MFA, session fixation, credential stuffing → `loom-auth` |
| A08 | Software & Data Integrity | Unsigned artifacts, insecure deserialization, CI/CD supply chain |
| A09 | Logging & Monitoring Failures | No audit trail, secrets in logs, no alerting, tamperable logs |
| A10 | **SSRF** | Server fetches attacker-controlled URLs → internal/metadata (see below) |

For APIs, cross-check the **OWASP API Security Top 10** — API1 is BOLA (object-level authz), the single most frequent API defect.

## Vulnerability Patterns (wrong → right)

```python
# A03 SQL injection
query = f"SELECT * FROM users WHERE id = {uid}"          # ✗ string interpolation
cursor.execute("SELECT * FROM users WHERE id = %s", (uid,))  # ✓ parameterized

# A03 Command injection
os.system(f"ping {host}")                                 # ✗
subprocess.run(["ping", "-c", "1", host])                 # ✓ arg vector, no shell

# A03 XSS
return f"<h1>Welcome {username}</h1>"                      # ✗
return f"<h1>Welcome {escape(username)}</h1>"             # ✓ context-aware output encoding

# A01 Path traversal
open(os.path.join(base, filename))                        # ✗ filename="../../etc/passwd"
p = os.path.normpath(os.path.join(base, filename))        # ✓ normalize then confine
if not p.startswith(base + os.sep): raise SecurityError

# A08 Insecure deserialization
pickle.loads(user_input)                                  # ✗ RCE by design
json.loads(user_input)                                    # ✓ data-only format
```

### A10 SSRF — defense is an allowlist, never a blocklist

Any server-side fetch of a user-influenced URL (webhooks, image proxies, PDF renderers, URL previews, importers) can be pointed at internal services or the cloud metadata endpoint `169.254.169.254` to steal IAM credentials.

```python
# ✗ blocklist — bypassed by DNS rebinding, redirects, encoded IPs
if host in ("localhost", "127.0.0.1"): reject()

# ✓ allowlist scheme + host; resolve then re-check; forbid redirects to non-allowlisted
if scheme not in {"https"} or host not in ALLOWED_HOSTS: reject()
ip = resolve(host)
if ip_is_private(ip) or host not in ALLOWED_HOSTS: reject()   # re-check post-DNS (rebinding)
fetch(url, allow_redirects=False, timeout=5)
```

Blocklists miss: DNS-rebinding, HTTP→internal redirects, IPv6 (`[::1]`, `[::ffff:127.0.0.1]`), and decimal/octal/hex IP encodings (`http://2130706433/` = `127.0.0.1`). Enforce IMDSv2 on AWS so a bare SSRF can't read credentials.

Access-control (IDOR/BOLA), timing-safe secret comparison, JWT algorithm confusion, and password-hash choice (argon2id, bcrypt 72-byte truncation) are covered with wrong-vs-right in **`loom-auth`** — audit against that checklist rather than duplicating here. Runtime secret leakage (argv/`ps`, env inheritance, logs/stack traces) is in **`loom-security-scan`**.

## Infrastructure & Cloud

- **IAM** — least privilege; no `*:*`; audit privilege-escalation paths (pass-role, policy-attach); no long-lived keys where roles work.
- **Storage** — no public buckets/blobs; encryption at rest (KMS-managed keys, not hardcoded); block public ACLs org-wide.
- **Network** — no `0.0.0.0/0` on 22/3389/DB ports; segment; security-group minimalism.
- **Metadata** — IMDSv2 required (SSRF hardening).
- **Containers** — non-root, read-only rootfs, drop capabilities, no `--privileged`, no secrets in image layers/env; scan images (`loom-security-scan`).
- **K8s** — RBAC least-privilege, NetworkPolicies default-deny, Pod Security Standards (restricted), etcd encryption, no `automountServiceAccountToken` unless needed.
- **IaC** — scan Terraform/CFN with tfsec/checkov (`loom-security-scan`); review before apply.

Handy audit probes (tool details in `loom-security-scan`):

```bash
aws s3api get-bucket-policy-status --bucket B          # public?
aws ec2 describe-security-groups \
  --query "SecurityGroups[?IpPermissions[?IpRanges[?CidrIp=='0.0.0.0/0']]]"
kubectl get networkpolicies -A                         # gaps = flat network
```

## Security Headers (baseline)

```nginx
add_header Strict-Transport-Security "max-age=31536000; includeSubDomains; preload" always;
add_header Content-Security-Policy "default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'" always;
add_header X-Content-Type-Options "nosniff" always;
add_header Referrer-Policy "strict-origin-when-cross-origin" always;
server_tokens off;   # don't leak version
```

CSP is the real XSS backstop; `X-Frame-Options`/`frame-ancestors 'none'` stops clickjacking; `X-XSS-Protection` is deprecated (omit or `0`).

## Data & ML

- **Data** — classify (PII/PHI/PCI); encrypt at rest + in transit; column/row-level access; audit-log access to sensitive data; automate PII detection/masking; enforce retention & secure deletion.
- **ML** — training-data provenance & poisoning checks; sign & access-control model artifacts (no embedded secrets); inference input validation + rate limits (anti-extraction); output filtering (no training-data/PII leakage); differential privacy where required. Attack taxonomy → `loom-threat-model`.

## Severity (CVSS-anchored)

| Level | CVSS | Examples | SLA |
| ----- | ---- | -------- | --- |
| Critical | 9.0–10 | RCE, auth bypass, SQLi w/ exfil, hardcoded master creds | Immediate |
| High | 7.0–8.9 | Privilege escalation, stored XSS in admin, insecure deser, missing authz on sensitive endpoint | 7 days |
| Medium | 4.0–6.9 | Info disclosure, CSRF, missing rate limit, weak password policy, known-CVE dep | 30 days |
| Low | 0.1–3.9 | Missing headers, verbose errors, missing cookie flags | Next release |
| Info | 0 | Hardening / defense-in-depth suggestions | Backlog |

Map findings to CWE/CVE/OWASP IDs so they're deduplicable and trackable.

## Compliance (essentials)

- **SOC2** — access control (MFA, quarterly access reviews, least privilege), encryption at rest/transit, monitoring/SIEM + alerting, incident-response runbooks, change management. It's controls + *evidence*.
- **PCI-DSS** — never store CAV2/CVC2/PIN; encrypt PAN, mask to last-4 in UI/logs; segment the CDE; audit trails for all cardholder-data access; quarterly scans + annual pentest.
- **HIPAA** — PHI encryption, access controls + audit logs, BAAs with processors, breach notification.
- **GDPR** — lawful basis/consent; data minimization; DSAR support: access, rectification, **erasure**, portability; breach notification **< 72 h**; pseudonymization; cross-border transfer safeguards. (Privacy threat modeling → LINDDUN in `loom-threat-model`.)

## Audit Checklist (verify before sign-off)

- [ ] Scope, assets, and compliance obligations documented up front
- [ ] Automated scans run and triaged first (`loom-security-scan`, `loom-dependency-scan`); human effort spent on logic/authz flaws
- [ ] A01 Access control audited object-by-object (IDOR/BOLA) — the top-frequency defect
- [ ] Injection, SSRF (allowlist), deserialization, path traversal checked with repro evidence
- [ ] Crypto: TLS ≥1.2, no weak/deprecated algos, secrets in KMS/vault (verify via `loom-auth`)
- [ ] Cloud/IaC/containers/K8s reviewed for least-privilege, no public exposure, IMDSv2
- [ ] Secrets absent from code, logs, argv, error responses
- [ ] Every finding: severity (CVSS), CWE/OWASP mapping, `file:line` evidence, concrete remediation
- [ ] Findings ranked most-severe first; exec summary + technical detail produced
- [ ] Applicable compliance controls mapped with evidence
