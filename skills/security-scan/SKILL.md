---
name: security-scan
description: Quick routine security checks for secrets, dependencies, container images, and common vulnerabilities. Run frequently during development. Triggers: security scan, SAST, DAST, vulnerability scan, dependency scan, container scan, secret scan, credential scan, quick scan, secrets check, vulnerability check, security check, pre-commit security, routine security, Snyk, Trivy, Semgrep, CodeQL, Bandit, safety, npm audit, cargo audit, gitleaks, trufflehog, govulncheck, pip-audit.
allowed-tools: Read, Grep, Glob, Bash
---

# Security Scan

## Overview

This skill provides quick, routine security checks that should be run frequently during development. These are lightweight scans designed to catch common issues early, not comprehensive audits.

## Tool Selection Matrix

| Scan Type | Best Tool | Alternative | Use Case |
|-----------|-----------|-------------|----------|
| **Secrets** | TruffleHog | Gitleaks | Hardcoded credentials, API keys |
| **Dependencies (JS)** | npm audit | Snyk, OWASP | Known CVEs in packages |
| **Dependencies (Python)** | pip-audit | safety | PyPI vulnerability database |
| **Dependencies (Go)** | govulncheck | nancy | Official Go vuln DB |
| **Dependencies (Rust)** | cargo audit | - | RustSec Advisory DB |
| **Container Images** | Trivy | Grype, Snyk | Image vulnerabilities, secrets |
| **SAST (Multi-lang)** | Semgrep | CodeQL | Security anti-patterns |
| **SAST (Python)** | Bandit | Semgrep | Python-specific issues |
| **SAST (Go)** | gosec | Semgrep | Go-specific issues |
| **Dockerfile** | hadolint | Trivy config | Best practices, misconfig |
| **IaC (Terraform)** | tfsec | Checkov, Trivy | Terraform misconfigurations |
| **IaC (K8s)** | kubesec | Trivy, Checkov | Kubernetes YAML security |

## When to Use

- **Before commits**: Quick check for secrets and obvious issues
- **During PR review**: Verify no new vulnerabilities introduced
- **Regular intervals**: Daily/weekly automated checks
- **After dependency updates**: Verify no new CVEs
- **Quick sanity checks**: Fast verification during development

For comprehensive security work, use the `security-audit` skill or invoke the `security-engineer` agent.

## Quick Scan Checklist

Run these checks in order of priority:

### 1. Secret Detection (Critical)

**Goal**: Find hardcoded credentials, API keys, tokens, and private keys before they reach version control.

```bash
# Check for hardcoded secrets with grep patterns
# API keys
grep -rn --include="*.{js,ts,py,go,java,rb,php}" \
  -E "(api[_-]?key|apikey)\s*[:=]\s*['\"][a-zA-Z0-9]{16,}" .

# AWS credentials
grep -rn --include="*.{js,ts,py,go,java,rb,php,env,yaml,yml,json}" \
  -E "(AKIA|ABIA|ACCA|ASIA)[A-Z0-9]{16}" .

# Private keys
grep -rn --include="*.{pem,key,env}" \
  -E "-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----" .

# Generic secrets
grep -rn --include="*.{js,ts,py,go,java,rb,php}" \
  -E "(password|secret|token)\s*[:=]\s*['\"][^'\"]{8,}" .
```

**Better: Use dedicated tools**

```bash
# TruffleHog (recommended)
trufflehog filesystem --directory=. --only-verified --no-update

# GitLeaks
gitleaks detect --source=. --no-git

# git-secrets (if installed)
git secrets --scan
```

### 2. Dependency Vulnerabilities (High)

**Goal**: Identify known CVEs in direct and transitive dependencies across package ecosystems.

```bash
# Node.js
npm audit --audit-level=high
# or
yarn audit --level high

# Python
pip-audit
# or
safety check

# Go
govulncheck ./...

# Rust
cargo audit

# Ruby
bundle audit check --update

# .NET
dotnet list package --vulnerable --include-transitive

# Multi-ecosystem (Snyk - requires account)
snyk test --severity-threshold=high

# OWASP Dependency-Check (slow but comprehensive)
dependency-check --scan . --failOnCVSS 7
```

### 3. Container Image Scanning (High)

**Goal**: Scan Docker images for vulnerabilities, misconfigurations, and embedded secrets.

```bash
# Trivy (recommended - fast, comprehensive)
trivy image --severity HIGH,CRITICAL myimage:latest
trivy image --scanners vuln,secret,config myimage:latest

# Grype (Anchore)
grype myimage:latest --only-fixed

# Snyk Container
snyk container test myimage:latest --severity-threshold=high

# Docker Scout (Docker Desktop)
docker scout cves myimage:latest --only-severity critical,high

# Clair (requires server)
clairctl analyze myimage:latest

# Scan Dockerfile before building
hadolint Dockerfile
trivy config --severity HIGH,CRITICAL Dockerfile
```

### 4. Quick Static Analysis (Medium)

**Goal**: Detect security anti-patterns and common vulnerability classes with SAST tools.

```bash
# Multi-language with Semgrep (fast defaults)
semgrep --config=p/security-audit --config=p/secrets .

# Python only
bandit -r . -ll  # Only high severity

# JavaScript/TypeScript
npx eslint . --ext .js,.ts --no-eslintrc \
  --plugin security --rule 'security/detect-object-injection: error'

# Go
gosec -severity high ./...

# CodeQL (requires GitHub setup)
codeql database create codeql-db --language=javascript
codeql database analyze codeql-db --format=sarif-latest --output=results.sarif

# SonarQube (requires server)
sonar-scanner -Dsonar.projectKey=myproject
```

### 5. Configuration Checks (Medium)

**Goal**: Validate infrastructure-as-code and configuration files for security misconfigurations.

```bash
# Docker
hadolint Dockerfile

# Terraform
tfsec . --minimum-severity HIGH

# Kubernetes
kubesec scan deployment.yaml

# General config
checkov -f config.yaml --check HIGH
```

## Pre-Commit Hook Setup

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/trufflesecurity/trufflehog
    rev: v3.63.0
    hooks:
      - id: trufflehog
        entry: trufflehog filesystem --no-update --fail --only-verified
        args: ["--directory=."]

  - repo: https://github.com/zricethezav/gitleaks
    rev: v8.18.0
    hooks:
      - id: gitleaks

  - repo: https://github.com/returntocorp/semgrep
    rev: v1.52.0
    hooks:
      - id: semgrep
        args: ["--config=p/secrets", "--error"]
```

## Tool Installation Quick Reference

```bash
# Secret scanning
brew install trufflesecurity/trufflehog/trufflehog
brew install gitleaks

# Dependency scanning
npm install -g npm-audit
pip install pip-audit safety
go install golang.org/x/vuln/cmd/govulncheck@latest
cargo install cargo-audit

# Container scanning
brew install trivy
brew install anchore/grype/grype

# SAST
brew install semgrep
pip install bandit
go install github.com/securego/gosec/v2/cmd/gosec@latest

# Infrastructure scanning
brew install hadolint tfsec
```

## CI/CD Integration

### GitHub Actions (Recommended)

```yaml
name: Security Scan

on:
  push:
    branches: [main]
  pull_request:

jobs:
  security-scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Secret Scan
        uses: trufflesecurity/trufflehog@main
        with:
          extra_args: --only-verified

      - name: Dependency Scan
        run: |
          npm audit --audit-level=high || true
          # Add other package managers as needed

      - name: SAST
        uses: returntocorp/semgrep-action@v1
        with:
          config: p/security-audit p/secrets

      - name: Container Scan
        uses: aquasecurity/trivy-action@master
        with:
          image-ref: myimage:${{ github.sha }}
          severity: HIGH,CRITICAL
          exit-code: 1
```

### GitLab CI

```yaml
security-scan:
  stage: test
  image: returntocorp/semgrep:latest
  script:
    - semgrep --config=p/security-audit --config=p/secrets .
  allow_failure: false

dependency-scan:
  stage: test
  image: node:latest
  script:
    - npm audit --audit-level=high
  allow_failure: false

container-scan:
  stage: test
  image: aquasec/trivy:latest
  script:
    - trivy image --severity HIGH,CRITICAL $CI_REGISTRY_IMAGE:$CI_COMMIT_SHA
```

### CircleCI

```yaml
version: 2.1

orbs:
  security: circleci/security@1.0

workflows:
  security-checks:
    jobs:
      - security/scan:
          severity: high
      - trivy/scan:
          image: myimage:latest
```

## Scan Result Interpretation

### Severity Levels

| Level | Action | Timeline |
|-------|--------|----------|
| **Critical** | Block merge, fix immediately | Hours |
| **High** | Should fix before merge | Days |
| **Medium** | Plan to fix | Sprint |
| **Low** | Track, fix opportunistically | Backlog |

### Common False Positives

**Secret Detection**:
- Test fixtures with fake keys
- Documentation examples
- Base64-encoded non-secrets
- UUIDs and random IDs

**Dependency Scans**:
- Dev-only dependencies
- Unused code paths
- Already-mitigated issues

### Triaging Results

```markdown
## Scan Results Triage

### Confirmed Issues
| Finding | Severity | File | Action |
|---------|----------|------|--------|
| Hardcoded API key | Critical | config.js:42 | Remove, rotate key |
| lodash CVE | High | package.json | Update to 4.17.21 |

### False Positives
| Finding | Reason | Action |
|---------|--------|--------|
| test_api_key | Test fixture | Add to .gitleaksignore |
| dev dependency CVE | Not in prod | Document acceptance |

### Accepted Risks
| Finding | Justification | Reviewer |
|---------|---------------|----------|
| Low CVE in CLI tool | Internal use only | @security |
```

## Quick Commands Reference

```bash
# One-liner: Quick secret + dependency check
npm audit --audit-level=high && gitleaks detect --no-git

# Python projects
pip-audit && bandit -r src/ -ll

# Go projects
govulncheck ./... && gosec -severity high ./...

# Rust projects
cargo audit && cargo clippy -- -W clippy::security

# Container security stack
trivy image --severity HIGH,CRITICAL myimage:latest && \
hadolint Dockerfile && \
trivy config --severity HIGH,CRITICAL .

# Full quick scan (all tools installed)
trufflehog filesystem . --only-verified && \
npm audit --audit-level=high && \
semgrep --config=p/security-audit --config=p/secrets . && \
trivy fs --severity HIGH,CRITICAL .

# Comprehensive multi-ecosystem scan
snyk test --all-projects --severity-threshold=high
```

## Escalation

Escalate to full `security-audit` or `security-engineer` when:

- Critical findings discovered
- Unusual or complex vulnerabilities
- Architecture-level security concerns
- Compliance-related questions
- Incident response needed
