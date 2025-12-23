---
name: security-scan
description: Quick routine security checks for secrets, dependencies, and common vulnerabilities. Run frequently during development. Triggers: security scan, quick scan, secrets check, vulnerability check, security check, pre-commit security, routine security.
allowed-tools: Read, Grep, Glob, Bash
---

# Security Scan

## Overview

This skill provides quick, routine security checks that should be run frequently during development. These are lightweight scans designed to catch common issues early, not comprehensive audits.

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
```

### 3. Quick Static Analysis (Medium)

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
```

### 4. Configuration Checks (Medium)

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

## CI/CD Integration

### GitHub Actions

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

# Full quick scan (if tools installed)
trufflehog filesystem . --only-verified && \
npm audit --audit-level=high && \
semgrep --config=p/security-audit --config=p/secrets .
```

## Escalation

Escalate to full `security-audit` or `security-engineer` when:

- Critical findings discovered
- Unusual or complex vulnerabilities
- Architecture-level security concerns
- Compliance-related questions
- Incident response needed
