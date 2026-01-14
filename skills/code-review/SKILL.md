---
name: code-review
description: Performs comprehensive code reviews focusing on correctness, maintainability, performance, security, and best practices. Trigger keywords: review, code review, PR review, pull request, check code, audit code, feedback, approve, request changes, comment, suggestion, LGTM, nit, blocker, code quality, best practice, architecture review, design review, security review, infra review.
allowed-tools: Read, Grep, Glob, Bash
---

# Code Review

## Overview

This skill provides thorough code review capabilities across multiple domains, analyzing code for bugs, design issues, performance problems, security vulnerabilities, and adherence to best practices. It helps identify potential issues before they reach production.

## Agent Assignment

- **senior-software-engineer** (Opus) - Architecture, design patterns, complex logic review
- **security-engineer** (Opus) - Security vulnerabilities, authentication, authorization, data protection
- **senior-infrastructure-engineer** (Opus) - Infrastructure code (Terraform, K8s, Docker), deployment, scaling
- **software-engineer** (Sonnet) - Responds to review feedback, implements fixes

## Instructions

### 1. Gather Context

- Identify the files to review using Glob patterns
- Understand the project structure and conventions
- Check for existing linting/formatting rules

### 2. Analyze Code Structure

- Review file organization and module structure
- Check for proper separation of concerns
- Verify naming conventions are consistent

### 3. Check for Common Issues

- Logic errors and edge cases
- Error handling completeness
- Resource management (memory leaks, unclosed handles)
- Thread safety issues in concurrent code
- Input validation gaps

### 4. Evaluate Code Quality

- Readability and clarity
- DRY principle adherence
- SOLID principles compliance
- Appropriate abstraction levels
- Test coverage adequacy

### 5. Performance Review

- Algorithm complexity analysis
- Database query efficiency
- Memory usage patterns
- Caching opportunities

## Review Severity Levels

- **BLOCKER**: Must fix before merge - security issues, data loss, crashes
- **CRITICAL**: Should fix before merge - logic errors, major bugs, performance issues
- **MAJOR**: Fix soon - code quality, maintainability, tech debt
- **MINOR**: Nice to have - style preferences, suggestions, nits

## Domain-Specific Checklists

### Security Review Checklist

- **Authentication/Authorization**

  - Proper credential storage (hashed, salted passwords)
  - Session management (expiry, secure cookies, CSRF tokens)
  - Access control checks on all sensitive operations
  - OAuth/JWT token validation and expiry

- **Input Validation**

  - SQL injection prevention (parameterized queries)
  - XSS prevention (output encoding)
  - Command injection prevention (no shell execution with user input)
  - Path traversal prevention (sanitize file paths)
  - SSRF prevention (validate URLs)

- **Data Protection**

  - Sensitive data encryption at rest and in transit
  - PII handling compliance (GDPR, CCPA)
  - Secrets not in code or logs
  - Rate limiting on APIs
  - Secure random number generation (crypto-grade)

- **Dependencies**
  - No known vulnerable dependencies
  - Supply chain security (verify checksums)
  - Minimal attack surface

### Infrastructure Code Review Checklist

- **Terraform/IaC**

  - No hardcoded credentials or secrets
  - State file backend configured securely
  - Resource tagging for cost tracking
  - Proper IAM roles (principle of least privilege)
  - Network security (VPC, security groups, firewall rules)
  - Disaster recovery configuration (backups, multi-region)

- **Kubernetes**

  - Resource limits and requests defined
  - Pod security policies/admission controllers
  - Network policies for isolation
  - Secrets management (external secrets operator)
  - Health checks (liveness, readiness probes)
  - RBAC configured properly

- **Docker**

  - Minimal base images (distroless, alpine)
  - No secrets in layers
  - Multi-stage builds for size
  - Non-root user execution
  - Vulnerability scanning enabled

- **CI/CD**
  - Pipeline security (secrets injection, not in logs)
  - Build reproducibility
  - Deployment rollback capability
  - Testing in staging before production

### Data Pipeline Review Checklist

- **Data Quality**

  - Schema validation
  - Null handling strategy
  - Duplicate detection
  - Data type enforcement

- **Reliability**

  - Idempotent operations
  - Exactly-once processing guarantees
  - Dead letter queues for failures
  - Monitoring and alerting

- **Performance**

  - Batch processing where appropriate
  - Partitioning strategy
  - Compression usage
  - Query optimization

- **Cost**
  - Storage lifecycle policies
  - Compute resource right-sizing
  - Data transfer minimization

### ML Code Review Checklist

- **Model Development**

  - Reproducible experiments (seed setting)
  - Data versioning
  - Model versioning
  - Feature engineering documentation

- **Training**

  - Training/validation/test split correctness
  - Overfitting checks
  - Hyperparameter tracking
  - Gradient explosion/vanishing checks

- **Production**

  - Model serving latency requirements
  - Model monitoring (drift detection)
  - A/B testing capability
  - Rollback strategy

- **Ethics**
  - Bias detection in training data
  - Fairness metrics
  - Explainability/interpretability
  - Privacy preservation (differential privacy)

## Best Practices

1. **Be Specific**: Point to exact lines and provide concrete suggestions
2. **Prioritize Issues**: Use severity levels (BLOCKER, CRITICAL, MAJOR, MINOR)
3. **Explain Why**: Don't just say what's wrong, explain the reasoning
4. **Suggest Solutions**: Provide alternative implementations when possible
5. **Acknowledge Good Code**: Recognize well-written sections
6. **Consider Context**: Understand the constraints and trade-offs
7. **Be Constructive**: Frame feedback positively and professionally
8. **Focus on Impact**: Prioritize issues by user/business impact
9. **Reference Standards**: Link to style guides, security benchmarks, RFCs

## Examples

### Example 1: Reviewing a Python Function

```python
# Before Review
def process(data):
    result = []
    for item in data:
        if item['status'] == 'active':
            result.append(item['value'] * 2)
    return result

# Review Comments:
# 1. Function name is too generic - consider 'double_active_values'
# 2. No type hints - add typing for better maintainability
# 3. No docstring explaining purpose and parameters
# 4. No null/empty check on input data
# 5. Could use list comprehension for cleaner code

# After Review
def double_active_values(data: list[dict]) -> list[int]:
    """
    Doubles the values of all active items in the dataset.

    Args:
        data: List of dictionaries with 'status' and 'value' keys

    Returns:
        List of doubled values for active items
    """
    if not data:
        return []
    return [item['value'] * 2 for item in data if item.get('status') == 'active']
```

### Example 2: Security Review Flag

```javascript
// CRITICAL: SQL Injection vulnerability
const query = `SELECT * FROM users WHERE id = ${userId}`;

// Recommendation: Use parameterized queries
const query = "SELECT * FROM users WHERE id = ?";
db.query(query, [userId]);
```

### Example 3: Performance Review

```python
# Issue: O(n*m) complexity due to nested loops with list membership check
for user in users:
    if user.id in active_ids:  # O(n) lookup each time
        process(user)

# Recommendation: Convert to set for O(1) lookups
active_ids_set = set(active_ids)
for user in users:
    if user.id in active_ids_set:
        process(user)
```

### Example 4: Structured Review with Severity Levels

```markdown
# Code Review: auth/login.py

## BLOCKER Issues

### Line 45: SQL Injection Vulnerability

sql = f"SELECT \* FROM users WHERE email = '{email}'"
cursor.execute(sql)

**Issue**: User input directly interpolated into SQL query.
**Impact**: Attacker can execute arbitrary SQL commands.
**Fix**: Use parameterized queries:
cursor.execute("SELECT \* FROM users WHERE email = ?", (email,))

## CRITICAL Issues

### Line 78: Password Stored in Plain Text

user.password = request.form['password']

**Issue**: Password stored without hashing.
**Impact**: Database breach exposes all user passwords.
**Fix**: Use bcrypt or argon2:
from bcrypt import hashpw, gensalt
user.password = hashpw(password.encode('utf-8'), gensalt())

## MAJOR Issues

### Line 112: Missing Error Handling

user = User.query.filter_by(email=email).first()
send_email(user.email, token)

**Issue**: No check if user exists before accessing attributes.
**Impact**: AttributeError crash if user not found.
**Fix**: Add null check:
if user is None:
return {"error": "User not found"}, 404

## MINOR Issues

### Line 23: Inconsistent Naming

def GetUser(id): # PascalCase for function

**Issue**: Python convention is snake_case for functions.
**Fix**: Rename to `get_user(id)` for consistency.

## Positive Notes

- Good separation of validation logic (lines 30-40)
- Comprehensive unit test coverage
- Clear documentation on authentication flow
```

### Example 5: Infrastructure Review (Terraform)

```hcl
# BLOCKER: Line 12 - Hardcoded AWS Credentials
provider "aws" {
  access_key = "AKIAIOSFODNN7EXAMPLE"  # NEVER commit credentials
  secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
}

# Fix: Use AWS credential profiles or environment variables
provider "aws" {
  profile = var.aws_profile
  region  = var.aws_region
}

# CRITICAL: Line 45 - S3 Bucket Publicly Accessible
resource "aws_s3_bucket" "data" {
  bucket = "company-data"
  acl    = "public-read"  # Exposes all data publicly
}

# Fix: Make private and use bucket policies for controlled access
resource "aws_s3_bucket" "data" {
  bucket = "company-data"
  acl    = "private"
}

# MAJOR: Line 78 - Missing Backup Configuration
resource "aws_db_instance" "main" {
  # ... other config ...
  backup_retention_period = 0  # No backups
}

# Fix: Enable automated backups
backup_retention_period = 7
backup_window           = "03:00-04:00"
```
