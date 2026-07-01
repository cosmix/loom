---
name: loom-security-scan
description: Quick routine security checks for secrets, dependencies, container images, and common vulnerabilities. Use for lightweight pre-commit and CI scans with tools like Semgrep, Trivy, gitleaks, cargo audit, npm audit, and pip-audit. Not a substitute for deep audits (use loom-security-audit).
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
triggers:
  - security scan
  - SAST
  - DAST
  - vulnerability scan
  - dependency scan
  - container scan
  - secret scan
  - credential scan
  - quick scan
  - secrets check
  - vulnerability check
  - security check
  - pre-commit security
  - routine security
  - Snyk
  - Trivy
  - Semgrep
  - CodeQL
  - Bandit
  - safety
  - npm audit
  - cargo audit
  - gitleaks
  - trufflehog
  - govulncheck
  - pip-audit
---

# Security Scan

Fast, automatable checks to run pre-commit / in CI — catch secrets, known-CVE deps, image and IaC misconfig early. This is the *tooling* skill; for methodology and compliance use `loom-security-audit`, for STRIDE use `loom-threat-model`, for deep dependency/SBOM work use `loom-dependency-scan`.

## Tool Selection Matrix

| Scan type | Tool | Alternative | Notes |
| --------- | ---- | ----------- | ----- |
| Secrets | TruffleHog | Gitleaks | TruffleHog *verifies* live creds; Gitleaks is regex-fast |
| Deps (JS) | npm audit | osv-scanner, Snyk | `--audit-level=high` |
| Deps (Python) | pip-audit | safety | pip-audit uses OSV/PyPI advisory DB |
| Deps (Go) | govulncheck | osv-scanner | call-graph aware → fewer false positives |
| Deps (Rust) | cargo audit | cargo-deny | RustSec DB |
| Container image | Trivy | Grype | `--scanners vuln,secret,config` |
| SAST multi-lang | Semgrep | CodeQL | `p/security-audit`, `p/secrets` |
| SAST Python | Bandit | Semgrep | — |
| SAST Go | gosec | Semgrep | — |
| Dockerfile | hadolint | Trivy config | best-practice lint |
| IaC (Terraform) | tfsec / trivy config | Checkov | — |
| IaC (K8s) | kubesec / trivy | Checkov | — |
| Universal (fs+img, vuln+secret+config) | Trivy | osv-scanner | one binary for most CI needs |

## Priority Order

Run cheapest/highest-signal first; a secret in git history is worse than a medium CVE.

### 1. Secrets (Critical)

Prefer dedicated tools over grep — they cut false positives and TruffleHog verifies whether a key is *live*.

```bash
trufflehog filesystem . --only-verified --no-update   # only credentials confirmed active
gitleaks detect --source . --redact                   # scans full git HISTORY by default
```

⚠ Secret-scanning gotchas:

- **Scan history, not just the worktree.** A rotated key still lives in old commits and forks. `gitleaks detect` walks history; `gitleaks detect --no-git` / `trufflehog filesystem` only see current files. A committed-then-deleted secret must be **rotated**, not just removed — deletion doesn't scrub history.
- `--only-verified` (TruffleHog) suppresses unverifiable/expired hits — great for CI signal, but it will miss a secret whose endpoint it can't reach; run a full pass periodically.
- Regex fallback for a quick look (dedicated tools are better): AWS keys `(AKIA|ASIA)[A-Z0-9]{16}`, PEM `-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----`, generic `(password|secret|token|api[_-]?key)\s*[:=]\s*['"][^'"]{8,}`.

⚠ Secrets leak at **runtime**, beyond git — scanners won't catch these; flag them in review:

- **Process argv** — a secret passed as a CLI arg (`mytool --token=abc`) is world-visible in `ps`/`/proc/*/cmdline`. Pass via env or stdin/file instead.
- **Env inheritance** — child processes inherit the parent's env; a spawned subprocess or crash-reporter can exfiltrate `AWS_SECRET_ACCESS_KEY`. Scope/scrub env before spawning untrusted code.
- **Logs & error/stack traces** — request bodies, `Authorization` headers, DB URLs with passwords, and exception dumps routinely leak secrets. Redact structured fields; never log full request objects.
- **Client-side & URLs** — secrets in query strings land in access logs, referers, and browser history; `.env`/`NEXT_PUBLIC_*` bundled into frontend builds ship to users.

### 2. Dependency CVEs (High)

```bash
npm audit --audit-level=high        # JS   (yarn audit --level high)
pip-audit                           # Python
govulncheck ./...                   # Go   (reachability-aware)
cargo audit                         # Rust
osv-scanner -r .                    # multi-ecosystem, lockfile-driven
```

For triage, SBOM, license, and supply-chain (typosquat/dependency-confusion) → `loom-dependency-scan`. ⚠ `npm audit` reports advisories against the *lockfile* including transitive/dev-only deps — a "critical" in a build-time devDependency may be unreachable at runtime; confirm reachability before blocking a release.

### 3. Container Images (High)

```bash
trivy image --severity HIGH,CRITICAL --scanners vuln,secret,config myimage:tag
hadolint Dockerfile
trivy config --severity HIGH,CRITICAL Dockerfile   # pre-build misconfig lint
```

⚠ Scan the exact **immutable digest** (`myimage@sha256:…`) you'll deploy, not a floating `:latest` (mutable → scan/deploy drift). Rebuild on base-image CVEs; a passing scan goes stale as new CVEs land.

### 4. SAST (Medium)

```bash
semgrep --config=p/security-audit --config=p/secrets .   # fast, low-FP defaults
bandit -r . -ll                                          # Python, high-severity only
gosec -severity high ./...                               # Go
```

### 5. IaC / Config (Medium)

```bash
tfsec . --minimum-severity HIGH
kubesec scan deployment.yaml
checkov -d . --compact
```

## CI Integration

Set nonzero exit → fail the job. ⚠ `npm audit ... || true` never fails CI (swallows exit code) — only use `|| true` for advisory-only steps you deliberately don't gate on.

```yaml
# GitHub Actions
jobs:
  security:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }          # full history for secret scan
      - uses: trufflesecurity/trufflehog@main
        with: { extra_args: --only-verified }
      - uses: returntocorp/semgrep-action@v1
        with: { config: p/security-audit p/secrets }
      - uses: aquasecurity/trivy-action@master
        with: { scan-type: fs, severity: HIGH,CRITICAL, exit-code: 1 }
```

```yaml
# Pre-commit (.pre-commit-config.yaml) — block secrets before they're committed
repos:
  - repo: https://github.com/gitleaks/gitleaks
    rev: v8.18.0
    hooks: [{ id: gitleaks }]
  - repo: https://github.com/returntocorp/semgrep
    rev: v1.52.0
    hooks: [{ id: semgrep, args: ["--config=p/secrets", "--error"] }]
```

## Interpreting Results

| Severity | Action | Timeline |
| -------- | ------ | -------- |
| Critical | Block merge, fix now | Hours |
| High | Fix before merge | Days |
| Medium | Plan a fix | Sprint |
| Low | Track | Backlog |

Common false positives — triage, don't blindly suppress: test fixtures / example creds (add to `.gitleaksignore` with a note), unreachable/dev-only dep CVEs (document acceptance), base64/UUIDs mistaken for secrets. Record accepted risks with a justification and reviewer so the next scan doesn't re-litigate them.

## Verification Checklist

- [ ] Secret scan covers **git history** (not just worktree); any historical hit → key rotated, not just deleted
- [ ] Dependency scan run for **every** ecosystem in the repo (a Python service with a JS build tool needs both)
- [ ] Container scan targets the deployed **digest**; Dockerfile linted (hadolint + trivy config)
- [ ] SAST run with a security ruleset (not just style)
- [ ] CI steps **fail** on Critical/High (no stray `|| true`); pre-commit blocks secrets locally
- [ ] Findings triaged: confirmed / false-positive / accepted-risk, each with owner + justification
- [ ] Critical/complex/architecture findings escalated → `loom-security-audit`

## Escalate to `loom-security-audit`

Critical or novel findings, architecture-level concerns, compliance questions (SOC2/PCI/HIPAA/GDPR), or incident response — scanners find known patterns; humans/audits find logic and authorization flaws.
