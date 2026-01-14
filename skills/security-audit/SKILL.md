---
name: security-audit
description: Performs comprehensive security audits identifying vulnerabilities, misconfigurations, and security best practice violations across applications, APIs, infrastructure, and data pipelines. Covers OWASP Top 10, compliance requirements (SOC2, PCI-DSS, HIPAA, GDPR), penetration testing, vulnerability assessment, risk assessment, security reviews, and hardening. Trigger keywords: security audit, vulnerability assessment, penetration test, pentest, OWASP, CVE, security review, risk assessment, compliance, SOC2, PCI-DSS, HIPAA, GDPR, security checklist, threat modeling, attack surface, security posture, vulnerability scan, security hardening, security baseline, security controls, security gap analysis, infrastructure security, API security, cloud security, container security, Kubernetes security, network security, application security, data security, ML model security.
allowed-tools: Read, Grep, Glob, Bash
---

# Security Audit

## Overview

This skill provides comprehensive security auditing capabilities to identify vulnerabilities, misconfigurations, and security best practice violations across:

- Application code (OWASP Top 10, injection flaws, authentication issues)
- APIs (REST, GraphQL, gRPC security patterns)
- Infrastructure (cloud configs, IaC, container security)
- Data pipelines (data flow security, PII handling, encryption)
- ML models (adversarial attacks, model poisoning, data leakage)
- Compliance frameworks (SOC2, PCI-DSS, HIPAA, GDPR)

Use this skill for security reviews, vulnerability assessments, penetration testing preparation, risk assessments, and compliance audits.

## Instructions

### 1. Scope Assessment

- Identify assets to audit
- Determine compliance requirements
- Review security policies
- Plan audit methodology

### 2. Application Security Review

- Search for hardcoded secrets (API keys, credentials, tokens)
- Identify injection vulnerabilities (SQL, command, LDAP, XPath, NoSQL)
- Check authentication/authorization (session management, RBAC/ABAC)
- Review cryptographic implementations (algorithms, key management)
- Verify input validation and output encoding
- Check for OWASP Top 10 vulnerabilities
- Review error handling (information disclosure)
- Audit dependency vulnerabilities (CVEs, supply chain)

### 3. API Security Review

- Authentication mechanisms (OAuth2, JWT, API keys)
- Authorization checks on all endpoints
- Rate limiting and throttling
- Input validation (request body, headers, params)
- API versioning and deprecation handling
- CORS policies and origin validation
- GraphQL query complexity limits and depth restrictions
- gRPC authentication and authorization
- API documentation security (no sensitive data exposure)

### 4. Infrastructure Security Review

- Cloud configuration audits (AWS, GCP, Azure, Kubernetes)
- IaC security scanning (Terraform, CloudFormation, Pulumi)
- Container security (image scanning, runtime policies)
- Network segmentation and firewall rules
- Secrets management (vaults, rotation policies)
- Service mesh security (mTLS, service-to-service auth)
- CI/CD pipeline security (supply chain, artifact signing)
- Logging and monitoring configuration

### 5. Data Pipeline Security Review

- Data classification and tagging
- PII/PHI handling and encryption
- Data access controls and audit logs
- Data retention and deletion policies
- Data flow mapping and lineage
- Encryption at rest and in transit
- Backup security and recovery procedures
- Cross-border data transfer compliance

### 6. ML Model Security Review

- Training data poisoning risks
- Model inversion attacks
- Adversarial input handling
- Model extraction protection
- Inference API security
- Data leakage in model outputs
- Privacy-preserving techniques (differential privacy, federated learning)
- Model versioning and deployment security

### 7. Compliance Assessment

- SOC2 controls mapping
- PCI-DSS requirements (if processing payments)
- HIPAA compliance (if handling health data)
- GDPR requirements (if EU data subjects)
- Data residency requirements
- Audit trail completeness
- Incident response procedures
- Security awareness training

### 8. Report Findings

- Categorize by severity (Critical, High, Medium, Low, Info)
- Map to compliance frameworks (CWE, CVE, OWASP)
- Provide remediation steps with code examples
- Prioritize fixes by risk and effort
- Document evidence (file paths, line numbers, screenshots)
- Calculate risk scores (CVSS where applicable)
- Create executive summary and technical details

## Best Practices

1. **Defense in Depth**: Multiple security layers (network, application, data)
2. **Least Privilege**: Minimum necessary permissions for users, services, and processes
3. **Secure Defaults**: Safe out-of-the-box settings, fail securely
4. **Input Validation**: Never trust user input, validate server-side
5. **Encryption**: Protect data at rest and in transit (TLS 1.2+, AES-256)
6. **Logging**: Comprehensive audit trails without sensitive data
7. **Updates**: Keep dependencies current, patch CVEs promptly
8. **Zero Trust**: Verify explicitly, assume breach, least privileged access
9. **Secure SDLC**: Security requirements, threat modeling, code review
10. **Incident Response**: Documented procedures, tested playbooks

## Examples

### Example 1: Common Vulnerability Patterns

```python
# CRITICAL: SQL Injection
# Vulnerable
query = f"SELECT * FROM users WHERE id = {user_id}"
cursor.execute(query)

# Secure
query = "SELECT * FROM users WHERE id = %s"
cursor.execute(query, (user_id,))

# CRITICAL: Command Injection
# Vulnerable
os.system(f"ping {hostname}")

# Secure
import shlex
subprocess.run(["ping", shlex.quote(hostname)])

# HIGH: Cross-Site Scripting (XSS)
# Vulnerable
return f"<h1>Welcome {username}</h1>"

# Secure
from markupsafe import escape
return f"<h1>Welcome {escape(username)}</h1>"

# HIGH: Path Traversal
# Vulnerable
file_path = f"/uploads/{filename}"
with open(file_path) as f:
    return f.read()

# Secure
import os
base_dir = "/uploads"
safe_path = os.path.normpath(os.path.join(base_dir, filename))
if not safe_path.startswith(base_dir):
    raise SecurityError("Path traversal detected")

# MEDIUM: Insecure Deserialization
# Vulnerable
import pickle
data = pickle.loads(user_input)

# Secure
import json
data = json.loads(user_input)

# MEDIUM: Hardcoded Secrets
# Vulnerable
API_KEY = "sk-1234567890abcdef"

# Secure
API_KEY = os.environ.get("API_KEY")
```

### Example 2: Security Checklist

```markdown
## Authentication & Authorization

- [ ] Passwords hashed with bcrypt/argon2 (cost factor >= 10)
- [ ] MFA available for sensitive operations
- [ ] Session tokens are cryptographically random
- [ ] Session invalidation on logout
- [ ] Rate limiting on login attempts
- [ ] Account lockout after failed attempts

## Input Validation

- [ ] All inputs validated server-side
- [ ] Parameterized queries for all database operations
- [ ] Output encoding for HTML contexts
- [ ] File upload validation (type, size, content)
- [ ] URL validation and sanitization

## Cryptography

- [ ] TLS 1.2+ enforced for all connections
- [ ] Strong cipher suites only
- [ ] Certificates from trusted CAs
- [ ] Secrets stored in secure vault
- [ ] No deprecated algorithms (MD5, SHA1, DES)

## Access Control

- [ ] Principle of least privilege applied
- [ ] RBAC/ABAC properly implemented
- [ ] Resource authorization checked on every request
- [ ] Admin interfaces protected and audited

## Data Protection

- [ ] Sensitive data encrypted at rest
- [ ] PII handling compliant with regulations
- [ ] Data retention policies implemented
- [ ] Secure data deletion procedures

## Logging & Monitoring

- [ ] Security events logged
- [ ] Logs protected from tampering
- [ ] Alerting on suspicious activities
- [ ] Log retention meets compliance
```

### Example 3: Security Scanning Commands

```bash
# Secret scanning with trufflehog
trufflehog filesystem --directory=. --only-verified

# Dependency vulnerability scanning
npm audit --production
pip-audit
cargo audit

# Static analysis
semgrep --config=auto .
bandit -r src/

# Container scanning
trivy image myapp:latest
grype myapp:latest

# Infrastructure scanning
checkov -d terraform/
tfsec terraform/

# OWASP ZAP API scan
zap-api-scan.py -t https://api.example.com/openapi.json -f openapi

# SSL/TLS testing
testssl.sh https://example.com

# Kubernetes security
kubesec scan deployment.yaml
kube-bench run --targets=node
```

### Example 4: Security Headers Configuration

```nginx
# Nginx security headers
add_header X-Frame-Options "DENY" always;
add_header X-Content-Type-Options "nosniff" always;
add_header X-XSS-Protection "1; mode=block" always;
add_header Referrer-Policy "strict-origin-when-cross-origin" always;
add_header Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self';" always;
add_header Strict-Transport-Security "max-age=31536000; includeSubDomains; preload" always;
add_header Permissions-Policy "geolocation=(), microphone=(), camera=()" always;

# Hide server version
server_tokens off;
```

### Example 5: API Security Audit Checklist

```markdown
## REST API Security

Authentication & Authorization:
- [ ] JWT tokens validated on every request
- [ ] Token expiration enforced (access: 15min, refresh: 7days)
- [ ] Token revocation mechanism implemented
- [ ] API keys scoped with least privilege
- [ ] OAuth2 flows correctly implemented
- [ ] Authorization checked at resource level (not just endpoint)

Input Validation:
- [ ] Request body validated against schema (JSON Schema, Pydantic)
- [ ] Path parameters validated (type, format, range)
- [ ] Query parameters validated and sanitized
- [ ] Headers validated (Content-Type, Accept)
- [ ] File uploads validated (type, size, content scanning)
- [ ] URL parameters encoded to prevent injection

Rate Limiting & DoS Protection:
- [ ] Rate limiting per API key/user/IP
- [ ] Burst protection implemented
- [ ] Request size limits enforced
- [ ] Timeout policies configured
- [ ] Circuit breakers for downstream services

CORS & Origin Validation:
- [ ] CORS policies restrictive (not wildcard *)
- [ ] Allowed origins whitelisted
- [ ] Credentials flag used correctly
- [ ] Preflight requests handled securely

Error Handling:
- [ ] Generic error messages to clients
- [ ] Stack traces never exposed
- [ ] Error codes documented without leaking internals
- [ ] Sensitive data redacted from logs

GraphQL Specific:
- [ ] Query depth limiting (max 7-10 levels)
- [ ] Query complexity scoring implemented
- [ ] Introspection disabled in production
- [ ] Batch query limits enforced
- [ ] Field-level authorization implemented

gRPC Specific:
- [ ] mTLS for service-to-service communication
- [ ] Interceptors for authentication/authorization
- [ ] Message size limits configured
- [ ] Streaming RPCs have timeout/cancellation
- [ ] Reflection service disabled in production
```

### Example 6: Infrastructure Security Audit

```bash
# AWS security audit
aws iam get-account-password-policy
aws s3api get-bucket-encryption --bucket mybucket
aws ec2 describe-security-groups --query 'SecurityGroups[?IpPermissions[?FromPort==`22` && ToPort==`22` && IpRanges[?CidrIp==`0.0.0.0/0`]]]'
aws cloudtrail describe-trails
aws kms list-keys

# Kubernetes security audit
kubectl get pods --all-namespaces -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.containers[*].securityContext}{"\n"}{end}'
kubectl get networkpolicies --all-namespaces
kubectl get psp  # Pod Security Policies
kubectl get serviceaccounts --all-namespaces -o json | jq '.items[] | select(.automountServiceAccountToken != false)'

# Terraform security scanning
tfsec . --format=json --out=tfsec-results.json
checkov -d . --framework terraform --output-file-path . --output json

# Container image scanning
trivy image --severity HIGH,CRITICAL myapp:latest
grype myapp:latest -o json
docker scan myapp:latest

# SAST scanning
semgrep --config=p/owasp-top-ten --config=p/security-audit .
bandit -r . -f json -o bandit-results.json
```

### Example 7: Data Pipeline Security Review

```markdown
## Data Flow Security

Data Classification:
- [ ] Data classified (Public, Internal, Confidential, Restricted)
- [ ] PII/PHI identified and tagged
- [ ] Sensitive data inventory maintained
- [ ] Data retention policies defined per classification

Encryption:
- [ ] Data encrypted at rest (AES-256 or equivalent)
- [ ] Data encrypted in transit (TLS 1.2+)
- [ ] Key management via KMS/vault (not hardcoded)
- [ ] Key rotation policies implemented
- [ ] Encryption verified at each pipeline stage

Access Controls:
- [ ] Role-based access to data sources
- [ ] Service accounts with least privilege
- [ ] Data access logged and monitored
- [ ] Column-level security for sensitive fields
- [ ] Row-level security where applicable

Data Validation:
- [ ] Schema validation at ingestion
- [ ] Data quality checks prevent malicious inputs
- [ ] Anomaly detection for unusual patterns
- [ ] PII detection and masking automated

Compliance:
- [ ] GDPR: Right to erasure implemented
- [ ] GDPR: Data minimization applied
- [ ] GDPR: Consent tracking for EU subjects
- [ ] HIPAA: BAA with third-party processors
- [ ] CCPA: Do Not Sell mechanism implemented
- [ ] Data residency requirements met

Audit Trail:
- [ ] Data lineage tracked end-to-end
- [ ] Access logs immutable and retained
- [ ] Change tracking for schema/permissions
- [ ] Anomaly detection and alerting
```

### Example 8: ML Model Security Audit

```markdown
## ML Security Threats

Training Phase:
- [ ] Training data provenance verified
- [ ] Data poisoning detection (outlier detection, statistical tests)
- [ ] Training environment isolated and hardened
- [ ] Model checkpoints encrypted and access-controlled
- [ ] Training logs sanitized (no PII/secrets)

Model Artifacts:
- [ ] Models versioned and signed
- [ ] Model registry access-controlled
- [ ] Model artifacts scanned for embedded secrets
- [ ] Model provenance tracked (data, hyperparameters, code)

Inference Phase:
- [ ] Input validation and sanitization
- [ ] Adversarial input detection
- [ ] Rate limiting on inference API
- [ ] Output filtering (prevent data leakage)
- [ ] Inference logs sanitized

Attack Vectors:
- [ ] Model inversion attacks: Cannot reconstruct training data from model
- [ ] Membership inference: Cannot determine if data was in training set
- [ ] Model extraction: API rate limits prevent model stealing
- [ ] Evasion attacks: Adversarial robustness tested
- [ ] Backdoor attacks: Model audited for hidden triggers

Privacy Preservation:
- [ ] Differential privacy applied (if required)
- [ ] Federated learning for sensitive data (if applicable)
- [ ] Synthetic data used for testing
- [ ] PII removed from training data or anonymized

Monitoring:
- [ ] Model drift detection
- [ ] Inference anomaly detection
- [ ] Performance degradation alerting
- [ ] Security event logging
```

### Example 9: Compliance Mapping

```markdown
## SOC2 Type II Controls

CC6.1 Logical and Physical Access Controls:
- [ ] MFA enforced for all users
- [ ] Password complexity requirements
- [ ] Account lockout after failed attempts
- [ ] Access reviews quarterly
- [ ] Privileged access monitored

CC6.6 Logical Access - Encryption:
- [ ] Data encrypted at rest (AES-256)
- [ ] Data encrypted in transit (TLS 1.2+)
- [ ] Key management documented
- [ ] Encryption verified in audits

CC7.2 System Monitoring - Detection:
- [ ] SIEM deployed and configured
- [ ] Intrusion detection system active
- [ ] Log aggregation and retention
- [ ] Alerting on security events
- [ ] Incident response playbooks

## PCI-DSS Requirements

Requirement 3: Protect Stored Cardholder Data
- [ ] Cardholder data encrypted (AES-256)
- [ ] Encryption keys managed securely
- [ ] Card data retention minimized
- [ ] PAN masked in logs/UI (show last 4 only)

Requirement 6: Develop Secure Systems
- [ ] Secure coding guidelines followed
- [ ] Code review for security
- [ ] Web application firewall deployed
- [ ] Vulnerability scanning quarterly
- [ ] Penetration testing annually

Requirement 10: Track and Monitor Access
- [ ] Audit trails for all access to cardholder data
- [ ] Logs protected from modification
- [ ] Log retention 90 days immediate, 1 year archive
- [ ] Daily log review process

## GDPR Compliance

Data Subject Rights:
- [ ] Right to access: Export user data API
- [ ] Right to rectification: Update mechanisms
- [ ] Right to erasure: Delete all user data
- [ ] Right to portability: Machine-readable export
- [ ] Right to object: Opt-out mechanisms

Privacy by Design:
- [ ] Data minimization applied
- [ ] Purpose limitation enforced
- [ ] Storage limitation policies
- [ ] Privacy impact assessments completed
- [ ] DPO designated (if required)

Security Measures:
- [ ] Pseudonymization where possible
- [ ] Encryption for sensitive data
- [ ] Regular security testing
- [ ] Breach notification procedures (<72 hours)
```

### Example 10: Vulnerability Severity Matrix

```markdown
## Severity Classification

CRITICAL (CVSS 9.0-10.0):
- Remote code execution (RCE)
- SQL injection with data exfiltration
- Authentication bypass
- Hardcoded master credentials
- Unrestricted file upload with execution

Action: Patch immediately, emergency change process

HIGH (CVSS 7.0-8.9):
- Privilege escalation
- Cross-site scripting (XSS) in admin panel
- Insecure deserialization
- Missing authentication on sensitive endpoints
- Exposed admin interfaces

Action: Patch within 7 days

MEDIUM (CVSS 4.0-6.9):
- Information disclosure (stack traces, versions)
- CSRF on non-critical operations
- Missing rate limiting
- Weak password policies
- Outdated dependencies with CVEs

Action: Patch within 30 days

LOW (CVSS 0.1-3.9):
- Missing security headers
- Verbose error messages
- Directory listing enabled
- HTTP methods not restricted
- Cookie security flags missing

Action: Patch in next release

INFO (CVSS 0.0):
- Security best practice recommendations
- Hardening opportunities
- Defense in depth suggestions
```
