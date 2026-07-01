---
name: loom-threat-model
description: Threat modeling methodologies (STRIDE, DREAD, PASTA, attack trees) for secure architecture design. Use when planning new systems, reviewing architecture security, mapping trust boundaries and data flows, identifying threats, or assessing risk.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
  - WebFetch
triggers:
  - threat modeling
  - threat model
  - STRIDE
  - DREAD
  - PASTA
  - LINDDUN
  - attack tree
  - attack surface
  - trust boundary
  - data flow diagram
  - DFD
  - threat analysis
  - risk assessment
  - adversary
  - threat actor
  - threat vector
  - mitigation
  - security architecture
  - attack scenario
  - defense in depth
---

# Threat Modeling

Structured identification of what can go wrong in a *design*, before code exists. Answers four questions (Shostack): **What are we building? What can go wrong? What are we doing about it? Did we do a good job?** This skill is architecture-time analysis — for finding vulns in existing code use `loom-security-scan`/`loom-security-audit`; for auth mechanism details use `loom-auth`.

## When

New system design, architecture review, significant feature or trust-boundary change, third-party integration, or compliance evidence. Re-run when the architecture changes — a threat model is a living document, not a one-time deliverable.

## Methodologies

### STRIDE — the default; apply *per element* of the DFD

The core technique isn't "brainstorm STRIDE" — it's walking each DFD element and each data flow crossing a trust boundary, asking which STRIDE categories apply to *that* element.

| Threat | Violates | Typical control |
| ------ | -------- | --------------- |
| **S**poofing | Authentication | Strong authn, mTLS, signed tokens |
| **T**ampering | Integrity | Signatures, HMAC, input validation, WORM logs |
| **R**epudiation | Non-repudiation | Audit logs, signed receipts |
| **I**nformation disclosure | Confidentiality | Encryption, least-privilege, error hygiene |
| **D**enial of service | Availability | Rate limits, quotas, timeouts, autoscale |
| **E**levation of privilege | Authorization | AuthZ checks, sandboxing, least privilege |

Element→likely-STRIDE heuristic: external entities → S, R; processes → all six; data flows → T, I, D; data stores → T, I, D (and R if logs).

### DREAD — risk scoring (use with caution)

Score Damage, Reproducibility, Exploitability, Affected users, Discoverability (1–10); risk = mean. ⚠ DREAD is widely criticized as **subjective and inconsistent** across raters (Microsoft dropped it). Prefer a simple **Likelihood × Impact** matrix, or **CVSS** for concrete vulns, when you need defensible numbers. Whatever the scale, rank threats to drive mitigation order.

### PASTA / LINDDUN

- **PASTA** — 7-stage, risk-centric, business-objective-driven (objectives → tech scope → decomposition → threat analysis → vuln analysis → attack modeling → risk & mitigation). Use for high-stakes systems needing business alignment.
- **LINDDUN** — the STRIDE-equivalent for **privacy** threats (Linkability, Identifiability, Non-repudiation, Detectability, Disclosure, Unawareness, Non-compliance). Reach for it on GDPR/PII-heavy systems.

## Process

### 1. Scope & assets

List assets and *why an attacker wants them* — this drives everything.

| Asset | Classification | Impact if compromised |
| ----- | -------------- | --------------------- |
| Credentials/secrets | Confidential | Account/system takeover |
| Payment data | PCI-DSS | Financial + compliance loss |
| PII | GDPR | Privacy breach, fines |

### 2. DFD with trust boundaries (the load-bearing step)

**Threats concentrate at trust boundaries** — every arrow crossing one is where authn/authz/validation must live. Mark: external entities, processes, data stores, data flows, and boundaries (internet↔DMZ, DMZ↔internal, tenant↔tenant, host↔container, user↔kernel).

```text
[User] --HTTPS--> ║ DMZ ║ [API Gateway] --> ║ Internal ║ [App] --> [DB]
                  ↑ boundary: authn, TLS       ↑ boundary: authz, network policy, mTLS
```

A boundary you didn't draw is a check you won't add. Enumerate every flow: source, destination, protocol, data classification, boundary crossed.

### 3. Enumerate threats (STRIDE per element)

```markdown
### API Gateway
| STRIDE | Threat | Likelihood | Impact |
| ------ | ------ | ---------- | ------ |
| S | Forged/`alg:none` JWT | Med | High |
| I | Verbose error leaks stack/version | High | Med |
| D | Rate-limit bypass | Med | High |
| E | BOLA/IDOR → other users' objects | High | Critical |
```

### 4. Attack trees (for high-value goals)

Decompose a goal into OR/AND paths; the cheapest leaf is the likely attack and shows where to spend defense.

```text
             Steal User Data (goal)
        ┌──────────┼──────────┐        (OR)
   Compromise    Exploit App   Social
   Credentials   Vulnerability Engineering
    ┌───┴───┐     ┌───┴───┐
 Phishing Brute  SQLi   BOLA
[L:H I:H][L:M]  [L:M I:C][L:H I:H]
```

### 5. Mitigations — track to closure

```markdown
| Threat | Mitigation | Priority | Status |
| ------ | ---------- | -------- | ------ |
| BOLA/IDOR | Object-level ownership checks (loom-auth) | P0 | Not started |
| SQLi | Parameterized queries | P0 | In progress |
| JWT forgery | Pin alg, validate aud/iss/exp (loom-auth) | P1 | Done |
```

Every high/critical threat needs a mitigation, an accepted-risk decision (with owner + justification), or a transfer. Silence = unmanaged risk.

## High-Signal Threats & Their Real Controls

- **BOLA / IDOR (broken object-level authz)** — #1 API threat: any object accessed by id must verify caller ownership/tenant, not just "logged in". Details + wrong-vs-right in `loom-auth`.
- **SSRF** — attacker makes your server fetch an internal URL (cloud metadata `169.254.169.254`, `localhost`, internal services). **Defense is an allowlist of permitted hosts/schemes — never a blocklist.** Blocklists are bypassed by DNS-rebinding, HTTP redirects, IPv6 (`[::1]`), and decimal/octal/hex-encoded IPs (`http://2130706433/`). Also disable unused URL schemes and block redirects to non-allowlisted hosts.
- **Injection (SQL/NoSQL/command/LDAP/XXE)** — parameterize/allowlist; treat every trust-boundary input as hostile.
- **Mass assignment** — client sets fields it shouldn't (`role`, `isAdmin`, `tenant_id`); bind explicit allowlists.
- **Cloud metadata & IAM escalation** — SSRF-to-credentials via the metadata endpoint; enforce IMDSv2, least-privilege roles.
- **Supply chain** — dependency confusion, typosquat, unsigned artifacts (see `loom-dependency-scan`).

## Component Quick-Reference

| Component | Watch for |
| --------- | --------- |
| Web app | XSS, CSRF, clickjacking, open redirect, session fixation |
| API | Broken authn/authz (BOLA/BFLA), mass assignment, rate-limit bypass, injection |
| Database | SQLi, privilege escalation, unencrypted data/backups |
| Auth | Credential stuffing, session fixation, token leakage, MFA bypass |
| File upload | Malware, path traversal, RCE via content, storage exhaustion |
| Cloud/infra | Public buckets, IAM escalation, SSRF→metadata, exposed control plane |
| Containers/K8s | Vulnerable base images, secrets in env/layers, privileged pods, RBAC gaps, container escape |
| 3rd-party/webhooks | API-key exposure, webhook spoofing (verify HMAC), supply-chain |
| ML systems | Data poisoning, evasion/adversarial input, model inversion/extraction, PII leakage in outputs |

## Threat Model Document Template

```markdown
# Threat Model: <System>   (v1.0, YYYY-MM-DD, author, reviewers)
1. Executive summary — key risks & recommendations
2. System description — purpose, architecture, data classification, trust boundaries
3. Assets — table (asset, classification, owner)
4. Analysis — DFD, STRIDE-per-component, attack trees
5. Risk assessment — table (threat, likelihood×impact or CVSS, level)
6. Mitigations — table (threat, mitigation, owner, timeline, status)
7. Residual risks — accepted risks + justification
8. Review schedule — trigger conditions to revisit
```

## Verification Checklist

- [ ] DFD drawn with **every trust boundary** marked; each boundary-crossing flow enumerated
- [ ] STRIDE applied **per element**, not brainstormed globally
- [ ] Assets tied to attacker motivation and classification
- [ ] Every high/critical threat has a mitigation, accepted-risk, or transfer with an owner
- [ ] BOLA/IDOR checked on every object-addressing endpoint; SSRF defenses are allowlist-based
- [ ] Risk ranking uses a defensible scale (Likelihood×Impact or CVSS, not raw DREAD gut-feel)
- [ ] Model versioned and has a re-review trigger; privacy threats covered (LINDDUN) if PII-heavy
