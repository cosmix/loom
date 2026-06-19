---
name: loom-terraform
description: Infrastructure as Code with Terraform/OpenTofu. Use for cloud resource provisioning, module development, state and backend management, multi-environment deployments (workspaces, tfvars), provider configuration, and refactoring/import workflows.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - terraform
  - tf
  - opentofu
  - tofu
  - infrastructure
  - IaC
  - infrastructure as code
  - provision
  - cloud
  - aws
  - azure
  - gcp
  - kubernetes
  - k8s
  - module
  - provider
  - state
  - backend
  - plan
  - apply
  - workspace
  - resource
  - data source
  - output
  - variable
  - locals
  - import
  - taint
  - destroy
---

# Terraform / OpenTofu

## Overview

This skill covers comprehensive Terraform and OpenTofu infrastructure management including:

- Module design and development patterns
- State management and backends
- Multi-environment strategies (workspaces, tfvars, directory structure)
- Security best practices (IAM, encryption, secrets)
- Provider configuration and version management
- Troubleshooting common issues
- Migration and refactoring patterns

**Preferred Tool: OpenTofu** - OpenTofu is the open-source fork of Terraform maintained by the Linux Foundation. Prefer `tofu` commands over `terraform` when available. The syntax and configuration are fully compatible.

**Primary User: senior-software-engineer** - This skill supports the senior-software-engineer agent for all Terraform architecture and implementation work (infrastructure focus).

## Instructions

### 1. Plan Infrastructure Architecture

- Define resource requirements and dependencies
- Plan network topology (VPCs, subnets, routing)
- Identify module boundaries and reusability patterns
- Consider multi-region/multi-AZ high availability design
- Plan for disaster recovery and backup strategies
- Document security requirements (IAM, encryption, network isolation)

### 2. Write Terraform Configuration

**Module Structure:**

```text
terraform/
├── modules/
│   ├── vpc/
│   │   ├── main.tf       # Resource definitions
│   │   ├── variables.tf  # Input variables
│   │   ├── outputs.tf    # Output values
│   │   └── versions.tf   # Provider version constraints
│   ├── eks/
│   └── rds/
├── environments/
│   ├── dev/
│   │   ├── main.tf
│   │   ├── terraform.tfvars
│   │   └── backend.tf
│   ├── staging/
│   └── prod/
├── .terraform.lock.hcl   # Provider version lock file
└── README.md
```

**Configuration Principles:**

- Structure code in reusable modules with clear interfaces
- Define all variables with descriptions and validation rules
- Configure providers with version constraints
- Set up remote state backend with locking
- Use locals for computed values and DRY patterns
- Document outputs with descriptions
- Add lifecycle rules where needed (prevent_destroy, ignore_changes)

### 3. Module Development Patterns

**Module Interface Design:**

```hcl
# modules/app-service/variables.tf
variable "name" {
  description = "Name of the application service"
  type        = string
  validation {
    condition     = length(var.name) > 0 && length(var.name) <= 32
    error_message = "Name must be between 1 and 32 characters."
  }
}

variable "environment" {
  description = "Environment (dev, staging, prod)"
  type        = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be dev, staging, or prod."
  }
}

variable "vpc_id" {
  description = "VPC ID where resources will be created"
  type        = string
}

variable "subnet_ids" {
  description = "List of subnet IDs for resource placement"
  type        = list(string)
  validation {
    condition     = length(var.subnet_ids) >= 2
    error_message = "At least 2 subnets required for high availability."
  }
}

variable "tags" {
  description = "Additional tags to apply to resources"
  type        = map(string)
  default     = {}
}
```

**Module Composition:**

```hcl
# modules/app-infrastructure/main.tf
module "vpc" {
  source = "../vpc"

  name               = var.name
  cidr_block         = var.vpc_cidr
  availability_zones = var.availability_zones
  tags               = local.common_tags
}

module "database" {
  source = "../rds"

  name            = "${var.name}-db"
  engine          = "postgres"
  engine_version  = "15.4"
  instance_class  = var.db_instance_class
  vpc_id          = module.vpc.vpc_id
  subnet_ids      = module.vpc.private_subnet_ids
  security_groups = [module.vpc.database_security_group_id]
  tags            = local.common_tags
}

module "app" {
  source = "../ecs-service"

  name                = var.name
  vpc_id              = module.vpc.vpc_id
  subnet_ids          = module.vpc.private_subnet_ids
  database_endpoint   = module.database.endpoint
  database_secret_arn = module.database.secret_arn
  tags                = local.common_tags
}
```

**Module Versioning:**

```hcl
# Use versioned modules for stability
module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"  # Allow patch updates, not minor

  # ...
}

# For internal modules, use git tags
module "internal" {
  source = "git::https://github.com/org/terraform-modules.git//vpc?ref=v1.2.3"

  # ...
}
```

### 4. State Management Strategies

**Remote State Backend Configuration:**

**AWS S3 with native locking:**

```hcl
# backend.tf
terraform {
  backend "s3" {
    bucket       = "myorg-terraform-state"
    key          = "prod/eks/terraform.tfstate"
    region       = "us-west-2"
    encrypt      = true
    kms_key_id   = "arn:aws:kms:us-west-2:123456789012:key/..."
    use_lockfile = true   # native S3 conditional-write locking; no DynamoDB
  }
}
```

`use_lockfile = true` acquires the lock via an S3 conditional `PutObject` of a `<key>.tflock` object — no separate table and no DynamoDB IAM permissions. In Terraform, `dynamodb_table`-based locking is deprecated and will be removed in a future minor version. To migrate, set `use_lockfile = true` AND keep `dynamodb_table` simultaneously until all operators/CI have transitioned, then drop `dynamodb_table`. Note: OpenTofu's S3 backend supports both native and DynamoDB locking and has NOT deprecated DynamoDB — this deprecation is Terraform-specific.

**Setup Commands:**

```bash
# Create the state bucket (native locking needs no DynamoDB table)
aws s3api create-bucket \
  --bucket myorg-terraform-state \
  --region us-west-2 \
  --create-bucket-configuration LocationConstraint=us-west-2

aws s3api put-bucket-versioning \
  --bucket myorg-terraform-state \
  --versioning-configuration Status=Enabled

aws s3api put-bucket-encryption \
  --bucket myorg-terraform-state \
  --server-side-encryption-configuration '{
    "Rules": [{
      "ApplyServerSideEncryptionByDefault": {
        "SSEAlgorithm": "aws:kms",
        "KMSMasterKeyID": "arn:aws:kms:..."
      }
    }]
  }'
```

To protect the state bucket itself, put `lifecycle { prevent_destroy = true }` on the `aws_s3_bucket` resource that provisions it (and enable versioning/object-lock) — `lifecycle` is a resource meta-argument and is invalid inside a `backend` block.

**Backend blocks cannot reference variables** — backend config is evaluated before variables/locals/data sources, so any `var.*`/`local.*` there errors at init. Use *partial configuration*: leave dynamic args out of the block and supply them at init via `-backend-config=KEY=VALUE` or a `.tfbackend` file. NEVER pass credentials via `-backend-config` (Terraform persists them into `.terraform/` and plan files) or hardcode them — supply backend credentials only through environment variables (e.g. `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`).

```bash
# tofu init -backend-config="bucket=$TF_STATE_BUCKET" \
#           -backend-config="key=prod/app.tfstate"
```

**State Operations:**

```bash
# List resources in state
tofu state list

# Show specific resource
tofu state show aws_vpc.main

# Move resource to different address
# Prefer a version-controlled `moved` block over this imperative command
tofu state mv aws_instance.old aws_instance.new

# Remove resource from state (doesn't destroy)
# Prefer a `removed { ... lifecycle { destroy = false } }` block over this
tofu state rm aws_instance.temp

# Import existing resource
# Prefer a declarative `import` block (reviewable plan) over this imperative command
tofu import aws_instance.example i-1234567890abcdef0

# Pull remote state for inspection
tofu state pull > state.json

# Push modified state (DANGEROUS - use carefully)
tofu state push state.json

# Replace provider address (after provider migration)
tofu state replace-provider registry.terraform.io/hashicorp/aws \
  registry.opentofu.org/hashicorp/aws
```

**State Locking:**

- Always use state locking to prevent concurrent modifications
- S3 native (`use_lockfile = true`) for AWS, GCS for Google Cloud, Azure Storage for Azure
- If lock is stuck, verify no operations running before forcing: `tofu force-unlock LOCK_ID`

**State Migration:**

```bash
# Migrate from local to remote backend
# 1. Configure backend in backend.tf
# 2. Initialize with migration
tofu init -migrate-state

# Migrate between backends
# 1. Update backend configuration
# 2. Initialize and accept migration
tofu init -migrate-state -backend-config="bucket=new-bucket"
```

### 5. Multi-Environment Management

#### Strategy 1 (RECOMMENDED): Directory Structure — Isolated Backends & Credentials

This is the right choice for dev/staging/prod separation. Each environment is a separate root module with its OWN backend bucket and IAM role, so switching environments requires a deliberate directory change — there is no way to mis-target prod from a dev shell.

```text
terraform/
├── modules/          # Shared modules
├── environments/
│   ├── dev/
│   │   ├── main.tf
│   │   ├── backend.tf      # bucket=myorg-dev-state, dev IAM role (no prod access)
│   │   ├── terraform.tfvars
│   │   └── .terraform.lock.hcl
│   ├── staging/
│   └── prod/              # backend.tf -> bucket=myorg-prod-state, prod IAM role
```

#### Strategy 2: Workspaces — ONLY for ephemeral variants within ONE access boundary

> ⚠️ **Workspaces are NOT an environment-isolation mechanism.** The official docs state they "are not appropriate for system decomposition or deployments requiring separate credentials and access controls." All workspaces in a directory share ONE backend, ONE authentication context, and ONE provider config — there is no prod/dev boundary, and a mistyped `tofu workspace select prod && tofu apply` hits production with no config-level guard. Use workspaces only for short-lived variants of the SAME infrastructure within a single access boundary (PR preview / ephemeral test environments).

```bash
# Acceptable use: ephemeral PR-preview environments, one access boundary
tofu workspace new pr-1234
tofu workspace select pr-1234

# Use workspace in configuration
locals {
  environment = terraform.workspace

  instance_count = {
    dev     = 1
    staging = 2
    prod    = 5
  }

  count = local.instance_count[local.environment]
}
```

#### Strategy 3: tfvars Files (Flexible)

```bash
# environments/dev.tfvars
environment      = "dev"
instance_type    = "t3.small"
min_size         = 1
max_size         = 3
enable_monitoring = false

# environments/prod.tfvars
environment      = "prod"
instance_type    = "m5.large"
min_size         = 3
max_size         = 10
enable_monitoring = true

# Apply with specific vars
tofu apply -var-file="environments/prod.tfvars"
```

### 6. Security Best Practices

**IAM and Least Privilege:**

```hcl
# Create role with specific permissions
resource "aws_iam_role" "app" {
  name = "${var.name}-app-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ec2.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy" "app" {
  name = "${var.name}-app-policy"
  role = aws_iam_role.app.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = [
        "s3:GetObject",
        "s3:ListBucket"
      ]
      Resource = [
        aws_s3_bucket.data.arn,
        "${aws_s3_bucket.data.arn}/*"
      ]
    }]
  })
}
```

**Secrets Management:**

```hcl
# Store secrets in AWS Secrets Manager
resource "aws_secretsmanager_secret" "db_password" {
  name                    = "${var.name}-db-password"
  recovery_window_in_days = 7

  kms_key_id = aws_kms_key.secrets.id
}

resource "aws_secretsmanager_secret_version" "db_password" {
  secret_id = aws_secretsmanager_secret.db_password.id
  secret_string = jsonencode({
    username = var.db_username
    password = random_password.db_password.result
  })
}

resource "random_password" "db_password" {
  length  = 32
  special = true
}

# Reference secret in application
resource "aws_ecs_task_definition" "app" {
  # ...

  container_definitions = jsonencode([{
    name  = "app"
    image = var.app_image

    secrets = [{
      name      = "DATABASE_PASSWORD"
      valueFrom = aws_secretsmanager_secret.db_password.arn
    }]
  }])
}

# Mark outputs as sensitive
output "database_password_arn" {
  description = "ARN of database password secret"
  value       = aws_secretsmanager_secret.db_password.arn
  sensitive   = true
}
```

**Encryption:**

```hcl
# KMS key for encryption
resource "aws_kms_key" "data" {
  description             = "KMS key for data encryption"
  deletion_window_in_days = 30
  enable_key_rotation     = true

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "Enable IAM User Permissions"
        Effect = "Allow"
        Principal = {
          AWS = "arn:aws:iam::${data.aws_caller_identity.current.account_id}:root"
        }
        Action   = "kms:*"
        Resource = "*"
      },
      {
        Sid    = "Allow service to use the key"
        Effect = "Allow"
        Principal = {
          Service = "s3.amazonaws.com"
        }
        Action = [
          "kms:Decrypt",
          "kms:GenerateDataKey"
        ]
        Resource = "*"
      }
    ]
  })
}

# S3 bucket with encryption
resource "aws_s3_bucket" "data" {
  bucket = "${var.name}-data"
}

resource "aws_s3_bucket_server_side_encryption_configuration" "data" {
  bucket = aws_s3_bucket.data.id

  rule {
    apply_server_side_encryption_by_default {
      kms_master_key_id = aws_kms_key.data.arn
      sse_algorithm     = "aws:kms"
    }
    bucket_key_enabled = true
  }
}

# RDS with encryption
resource "aws_db_instance" "main" {
  # ...
  storage_encrypted = true
  kms_key_id        = aws_kms_key.data.arn
}
```

**Network Security:**

```hcl
# Security group with minimal access
resource "aws_security_group" "app" {
  name        = "${var.name}-app-sg"
  description = "Security group for application servers"
  vpc_id      = var.vpc_id

  # Egress only
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(var.tags, {
    Name = "${var.name}-app-sg"
  })
}

# Specific ingress rules
resource "aws_security_group_rule" "app_from_alb" {
  type                     = "ingress"
  from_port                = 8080
  to_port                  = 8080
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.alb.id
  security_group_id        = aws_security_group.app.id
}

# Network ACLs for additional layer
resource "aws_network_acl" "private" {
  vpc_id     = var.vpc_id
  subnet_ids = var.private_subnet_ids

  ingress {
    protocol   = "tcp"
    rule_no    = 100
    action     = "allow"
    cidr_block = var.vpc_cidr
    from_port  = 443
    to_port    = 443
  }

  egress {
    protocol   = "tcp"
    rule_no    = 100
    action     = "allow"
    cidr_block = "0.0.0.0/0"
    from_port  = 443
    to_port    = 443
  }

  tags = var.tags
}
```

### 7. Provider Configuration

**Version Constraints:**

```hcl
# versions.tf
terraform {
  # Floor chosen for the primitives this config relies on:
  #   1.9  cross-object variable validation, check/import blocks (1.5)
  # Raise to >= 1.10.0 if using ephemeral values, >= 1.11.0 for write-only args /
  # native S3 locking. For OpenTofu: >= 1.7.0 (state encryption), >= 1.10.0 (S3 locking).
  required_version = ">= 1.9.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"  # 5.x.x, but not 6.0.0
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.23"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.11"
    }
  }
}

provider "aws" {
  region = var.region

  # Default tags applied to all resources
  default_tags {
    tags = {
      Environment = var.environment
      ManagedBy   = "terraform"
      Project     = var.project
    }
  }
}

provider "kubernetes" {
  host                   = module.eks.cluster_endpoint
  cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)

  exec {
    api_version = "client.authentication.k8s.io/v1beta1"
    command     = "aws"
    args        = ["eks", "get-token", "--cluster-name", module.eks.cluster_name]
  }
}
```

**Provider Aliases (Multi-Region/Account):**

```hcl
provider "aws" {
  alias  = "primary"
  region = "us-west-2"
}

provider "aws" {
  alias  = "replica"
  region = "us-east-1"
}

resource "aws_s3_bucket" "primary" {
  provider = aws.primary
  bucket   = "${var.name}-primary"
}

resource "aws_s3_bucket" "replica" {
  provider = aws.replica
  bucket   = "${var.name}-replica"
}
```

### 8. Advanced Patterns

**Dynamic Blocks:**

```hcl
resource "aws_security_group" "app" {
  name   = "${var.name}-sg"
  vpc_id = var.vpc_id

  dynamic "ingress" {
    for_each = var.ingress_rules
    content {
      from_port   = ingress.value.from_port
      to_port     = ingress.value.to_port
      protocol    = ingress.value.protocol
      cidr_blocks = ingress.value.cidr_blocks
      description = ingress.value.description
    }
  }
}
```

**For Expressions:**

```hcl
locals {
  # Create map from list
  subnet_map = {
    for subnet in aws_subnet.private :
    subnet.availability_zone => subnet.id
  }

  # Transform and filter
  production_instances = {
    for k, v in var.instances :
    k => v if v.environment == "production"
  }

  # Complex transformation
  instance_configs = [
    for i in range(var.instance_count) : {
      name = "${var.name}-${i}"
      az   = element(var.availability_zones, i)
      tags = merge(var.tags, {
        Index = i
      })
    }
  ]
}
```

**Count vs For_Each:**

```hcl
# BAD - count creates unstable addresses
resource "aws_instance" "app" {
  count = length(var.instance_names)

  ami           = var.ami_id
  instance_type = var.instance_type
  # If you remove instance_names[1], instance_names[2] becomes [1]
}

# GOOD - for_each creates stable addresses
resource "aws_instance" "app" {
  for_each = toset(var.instance_names)

  ami           = var.ami_id
  instance_type = var.instance_type
  tags = {
    Name = each.value
  }
}
```

**Lifecycle Rules:**

```hcl
resource "aws_instance" "app" {
  ami           = var.ami_id
  instance_type = var.instance_type

  lifecycle {
    # Create replacement before destroying
    create_before_destroy = true

    # Prevent accidental deletion
    prevent_destroy = true

    # Ignore changes to specific attributes
    ignore_changes = [
      tags,
      user_data
    ]
  }
}
```

**Depends_on (Explicit Dependencies):**

```hcl
resource "aws_iam_role_policy" "app" {
  name = "${var.name}-policy"
  role = aws_iam_role.app.id

  policy = jsonencode({...})
}

resource "aws_instance" "app" {
  ami           = var.ami_id
  instance_type = var.instance_type
  iam_instance_profile = aws_iam_instance_profile.app.name

  # Ensure policy is attached before creating instance
  depends_on = [aws_iam_role_policy.app]
}
```

### 9. Troubleshooting Common Issues

#### Issue: State Lock Timeout

```bash
# Problem: Another process has state locked
# Solution 1: Wait for other process to complete
# Solution 2: Verify no process running, then force unlock
tofu force-unlock <LOCK_ID>
```

#### Issue: Provider Plugin Errors

```bash
# Problem: Corrupted provider cache
# Solution: Clear ONLY the plugin cache and re-initialize
rm -rf .terraform/
tofu init

# Do NOT delete .terraform.lock.hcl — that discards validated checksum pins and
# can pull a different provider version on the next init. If the lock file is
# genuinely corrupt, regenerate (don't just delete) it across all target platforms:
tofu providers lock \
  -platform=linux_amd64 -platform=darwin_arm64 -platform=windows_amd64
```

#### Issue: Resource Already Exists

```bash
# Problem: Resource exists but not in state
# Solution: Import existing resource
tofu import aws_instance.example i-1234567890abcdef0

# Or: Remove from code and manage outside Terraform
```

#### Issue: Circular Dependency

```hcl
# Problem: Resources depend on each other
# Solution 1: Use separate apply steps
resource "aws_security_group" "app" {
  # ...
}

resource "aws_security_group_rule" "app_to_db" {
  type                     = "egress"
  source_security_group_id = aws_security_group.app.id
  security_group_id        = aws_security_group.db.id
}

# Solution 2: Restructure dependencies to be unidirectional
```

#### Issue: Sensitive Data in State

```bash
# Problem: Passwords/keys visible in state file
# `sensitive = true` only REDACTS UI output — the value is still plaintext in state.
# Real fixes (see "Expert Practices > Security"):
#   - write-only args (Terraform >= 1.11) + ephemeral resources (>= 1.10): never persisted
#   - OpenTofu >= 1.7 `encryption` block: client-side state+plan encryption at rest
#   - external store (Secrets Manager/Vault): pass only ARNs/references through Terraform
# Always use remote state with encryption; never commit state files to version control.
```

#### Issue: Forcing Resource Replacement (`taint` is deprecated)

```bash
# `terraform/tofu taint` is DEPRECATED (since Terraform v0.15.2): it mutates state
# immediately with no plan preview, so other operators can plan against a tainted
# resource before the impact is reviewed.
# Use -replace instead — it produces a reviewable plan preview before replacing:
tofu plan -replace="aws_instance.example"
tofu apply -replace="aws_instance.example"
```

#### Issue: Resource Drift

```bash
# Detect drift
tofu plan -refresh-only

# View current vs desired state
tofu show

# Refresh state to match reality
tofu apply -refresh-only

# Override drift (restore to code definition)
tofu apply
```

#### Issue: Module Source Changes

```bash
# Problem: Module source or version changed
# Solution: Re-initialize and upgrade
tofu init -upgrade

# Lock provider versions
tofu providers lock
```

#### Issue: Large Plan Output

```bash
# Filter plan output
tofu plan | grep "will be created"

# Save plan for review
tofu plan -out=tfplan
tofu show tfplan

# Show only specific resource types
tofu plan | grep "aws_instance"
```

#### Issue: Timeout Errors

```hcl
# Configure timeouts for long-running operations
resource "aws_db_instance" "main" {
  # ...

  timeouts {
    create = "60m"
    update = "60m"
    delete = "60m"
  }
}
```

### 10. Testing and Validation

**Pre-Apply Validation:**

```bash
# Format code
tofu fmt -recursive

# Validate configuration
tofu validate

# Security scanning (using tfsec)
tfsec .

# Cost estimation (using infracost)
infracost breakdown --path .

# Policy checking (using OPA)
terraform-compliance -f compliance/ -p .
```

**Plan Review Checklist:**

- All changes are expected
- No resources marked for deletion unintentionally
- Sensitive data not exposed in outputs
- Tags applied to all resources
- Encryption enabled where required
- IAM policies follow least privilege
- Network security groups are restrictive

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance from the official docs. The *why* matters as much as the rule.

### Anti-Patterns

**Never put a `provider` block in a reusable child module.** The docs are explicit: "A module intended to be called by one or more other modules must not contain any provider blocks," and "a module with its own provider configurations is not compatible with for_each, count, or depends_on" on the module call. The mechanism: a provider config and the resources it manages must be destroyed together, but the graph cannot guarantee that ordering once the call is multiplied/ordered. Declare `provider` blocks only in root modules; pass non-default (aliased) providers explicitly via the `providers` argument. **Aliased providers are NEVER inherited** — forgetting to pass one silently falls back to the default provider (wrong region/account). Child modules declare `required_providers` for version constraints only — no provider block, and no `backend` block (only one backend per configuration).

```hcl
# root/main.tf
provider "aws" { alias = "usw2", region = "us-west-2" }

module "app" {
  source    = "./modules/app"
  providers = { aws = aws.usw2 }
  for_each  = var.environments   # works only because the module has no provider block
}
# modules/app/versions.tf -> required_providers ONLY, no provider/backend block
```

**Keep module composition flat (single level of child modules).** Modules accept inputs, emit outputs, and know nothing about where state lives or how callers are structured. Compose flat modules at the root and wire them via input/output rather than nesting deep hierarchies (root → A → B → C), which obscure dependency flow, complicate provider passing, and make refactoring hazardous.

```hcl
module "vpc" { source = "./modules/vpc", cidr = var.vpc_cidr }
module "rds" {
  source     = "./modules/rds"
  vpc_id     = module.vpc.vpc_id            # dependency expressed via inputs
  subnet_ids = module.vpc.private_subnet_ids
}
```

### Idioms

**Don't repeat the resource type in the name label.** The address `aws_security_group.app` already carries the type, so `aws_security_group.app_security_group` says the noun twice. Use a short, role-descriptive noun; separate words with underscores, not dashes (`aws_instance.web`, not `aws_instance.ec2-web-server`). The `this`/`main` singleton convention is a *community* idiom (terraform-best-practices.com), NOT in the official style guide.

**Use `moved`/`removed`/`import` blocks, not the imperative CLI.** `terraform state mv`/`import`/`taint` are imperative: each operator must run them, they are not version-controlled, and they are not reproducible across environments. The block forms are validated, reviewable in PRs, and run in CI with no manual steps.

- `moved` (≥ 1.1) encodes renames. **Never delete a published `moved` block** — doing so makes configs referencing the old address plan a *delete* instead of a move. Chain renames across successive moves; retain all historical blocks.
- `removed` (≥ 1.7) with `lifecycle { destroy = false }` drops a resource from management WITHOUT destroying the real infrastructure.
- `import` (≥ 1.5) blocks make import plannable; pair with `-generate-config-out=FILE` to auto-generate matching HCL for brownfield adoption. OpenTofu 1.7+ adds `for_each` for bulk imports. Unlike `moved`, import blocks may be deleted after they succeed.

```hcl
moved   { from = aws_instance.server,    to = aws_instance.app_server }            # chain, never prune
removed { from = aws_db_instance.legacy, lifecycle { destroy = false } }           # keep real infra
import  { to   = aws_instance.app,       id = "i-1234567890abcdef0" }               # then -generate-config-out
```

**Commit `.terraform.lock.hcl`, and pre-populate multi-platform checksums.** `terraform init` records checksums only for the platform it ran on, so a teammate/CI on another OS/arch hits "no matching checksum". Lock all needed platforms before committing. The lock pins PROVIDERS only — module pinning lives in the module `version` constraint. A CI `init -upgrade` re-resolves from constraints and overwrites lock selections, silently unpinning providers.

```bash
tofu providers lock -platform=linux_amd64 -platform=darwin_amd64 \
                    -platform=darwin_arm64 -platform=windows_amd64
```

### Gotchas

**`count` index shift silently recreates the WRONG resources — prefer `for_each`.** Removing a non-tail element of a `count` list reindexes everything after it: removing `[1]` of three makes old-`[2]` become `[1]`, so Terraform destroys & recreates the resource you never touched — a data-loss footgun for RDS/EBS/subnets. Key `for_each` over a map/set by a STABLE identity instead. Caveats: `toset()` on a list silently dedupes duplicates (dropping instances), and a `moved` block migrates existing count-indexed state to for_each keys without recreation.

**`for_each` keys must be known at plan time and non-sensitive.** Keys appear in plan UI, so (1) sensitive values are categorically forbidden as `for_each` args (leaking a key leaks the secret), and (2) a computed/"known after apply" attribute (generated ID/ARN/endpoint) as a key is a plan-time error. Impure functions (`uuid()`, `timestamp()`, `bcrypt()`) are also disallowed — identity must be stable across runs. Derive a non-sensitive, statically-known key set with a `for` expression first.

**Data sources are read at plan time and return last-known values.** A `data` lookup reflects state as of the plan, not the live moment of apply. If any argument (or an added `depends_on`) references a "known after apply" value, the read is DEFERRED to apply and the plan shows "(known after apply)", making downstream plan review meaningless. Keep data-source args static; to use an attribute of a resource you are creating, **reference that resource's attribute directly** rather than re-looking it up through a data source.

**`create_before_destroy` is graph-wide and irreversible.** Terraform "propagates and applies create_before_destroy behavior to all resource dependencies" and stores it in state; you cannot override it to `false` on a dependency (that would imply a cycle). So one leaf change can silently alter replacement ordering of upstream infra. Two more traps: with CBD true, `destroy`-time provisioners do NOT run (drain/deregister logic is skipped); and unique-name resources (security groups, RDS, IAM, S3) collide during the create-then-destroy overlap — use `name_prefix` or a `random_id`/`random_pet` suffix.

```hcl
resource "aws_security_group" "app" {
  name_prefix = "${var.name}-app-"   # unique per replacement; avoids overlap collision
  lifecycle { create_before_destroy = true }
}
```

**`prevent_destroy` is bypassed when you delete the resource block.** It blocks destroy plans only WHILE the block exists; Terraform does not store the rule in state (unlike `create_before_destroy`). Deleting the block makes the next apply plan destruction of the live infra, with no guard to stop it. To stop managing a resource without destroying it, use a `removed` block with `destroy = false`.

### Performance

**Prefer implicit attribute references over `depends_on`.** Referencing an attribute (`aws_iam_instance_profile.app.name`) gives Terraform the exact dependency scope and full parallelism. `depends_on` is a blunt instrument: Terraform plans conservatively, marks more values "(known after apply)", and "can cause Terraform to create more conservative plans that replace more resources than necessary." Worst on a `module` call — it serializes ALL resources inside the module, even ones that need not wait. Reserve `depends_on` for hidden side effects with no referenceable attribute (e.g. an IAM policy that must propagate before bootstrap).

### Security

**`sensitive = true` does NOT protect state.** Docs: "Terraform stores values with the sensitive argument in both state and plan files, and anyone who can access those files can access your sensitive values." `terraform output -json`/`-raw` print them in plaintext regardless. The only mechanisms that keep a secret out of state are:

- **Write-only arguments** (Terraform ≥ 1.11, provider-specific, e.g. `password_wo` + `password_wo_version`) — the provider consumes them, Terraform never persists them.
- **Ephemeral resources/values** (≥ 1.10) — fetched per-phase and discarded before state/plan are written.

For older versions/providers, keep the secret in an external store and pass only ARNs/references.

```hcl
ephemeral "aws_secretsmanager_secret_version" "db" { secret_id = aws_secretsmanager_secret.db.id }
resource "aws_db_instance" "main" {
  password_wo         = ephemeral.aws_secretsmanager_secret_version.db.secret_string
  password_wo_version = 1   # increment to rotate
}
```

**Avoid `terraform_remote_state` across team boundaries.** It exposes only outputs, but the consumer must have read access to the ENTIRE state snapshot to get them — "any user or server which has enough access to read the root module output values will also always have access to the full state snapshot data," which often includes secrets. Renaming an output also breaks every consumer. Publish data explicitly to a neutral store (SSM Parameter Store, Consul, S3, DNS) so access controls on shared data and on state differ; HCP Terraform's `tfe_outputs` avoids full-state access. Within one team's own repo, `terraform_remote_state` is fine.

```hcl
resource "aws_ssm_parameter" "vpc_id" { name = "/shared/networking/vpc-id", type = "String", value = aws_vpc.main.id }
data "aws_ssm_parameter" "vpc_id"     { name = "/shared/networking/vpc-id" }   # consumer needs no state access
```

### Validation (Design Pattern)

Layered validation, each tier with a distinct scope and failure behavior:

- **Variable `validation`** — checks raw input shape/range. Since 1.9 it may reference other objects, but it CANNOT reach a `data` source / provider-returned attribute. Halts.
- **`precondition`/`postcondition`** (≥ 1.2, inside `lifecycle`) — run with resolved values: preconditions assert cross-resource invariants before create; postconditions validate provider-returned attributes after create. Both HALT on failure.
- **`check` blocks** (≥ 1.5) — run at the END of plan/apply and report failures as WARNINGS without halting. Use for health probes, cert-expiry, and compliance/drift that should surface but not gate a deploy; they can embed a scoped `data` source.

```hcl
data "aws_ami" "app" {
  lifecycle {
    postcondition {                                  # blocking invariant
      condition     = self.architecture == "x86_64"
      error_message = "AMI ${self.id} is ${self.architecture}; only x86_64 supported."
    }
  }
}
check "api_health" {                                 # non-blocking observation
  data "http" "endpoint" { url = "https://${aws_lb.app.dns_name}/health" }
  assert {
    condition     = data.http.endpoint.status_code == 200
    error_message = "Health endpoint returned ${data.http.endpoint.status_code}"
  }
}
```

### Currency

**OpenTofu offers client-side state encryption; OSS Terraform does not.** OpenTofu 1.7+ adds an `encryption` block inside `terraform {}` that encrypts state and plan files (AES-GCM, PBKDF2 passphrase or KMS key provider) before they leave the process — protecting secrets at rest independent of backend encryption (S3 SSE still leaves state readable to anyone with bucket access). Tradeoff/gotcha: tools that parse raw state (remote-state data sources, drift comparison, some third-party tooling) do not work against encrypted state, so plan for key distribution. Use a KMS key provider (not a static passphrase) in production for rotation and audit; never hardcode a passphrase.

```hcl
terraform {
  encryption {
    key_provider "aws_kms" "main" { kms_key_id = var.kms_key_id, region = var.region }
    method "aes_gcm" "default"    { keys = key_provider.aws_kms.main }
    state { method = method.aes_gcm.default }
    plan  { method = method.aes_gcm.default }
  }
}
```

## Best Practices Summary

1. **Module Design**: Create reusable, versioned modules with clear interfaces
2. **Remote State**: Always use encrypted remote state with locking
3. **Variables**: Parameterize everything, add validation rules
4. **Workspaces/Environments**: Choose strategy based on isolation needs
5. **Formatting**: Always run `tofu fmt` before committing
6. **Validation**: Use `tofu validate` and static analysis tools
7. **Plan Review**: Always review plan output before applying
8. **Security**: Enable encryption, use Secrets Manager, follow least privilege
9. **Tagging**: Tag all resources for cost allocation and management
10. **Documentation**: Document modules, variables, outputs, and architecture decisions
11. **Version Constraints**: Pin provider versions for reproducibility
12. **State Operations**: Use state commands carefully, understand implications
13. **Testing**: Test modules in isolation before using in production
14. **Drift Detection**: Regularly check for drift with `tofu plan -refresh-only`

## OpenTofu vs Terraform Commands

| Terraform             | OpenTofu (Preferred) |
| --------------------- | -------------------- |
| `terraform init`      | `tofu init`          |
| `terraform plan`      | `tofu plan`          |
| `terraform apply`     | `tofu apply`         |
| `terraform destroy`   | `tofu destroy`       |
| `terraform fmt`       | `tofu fmt`           |
| `terraform validate`  | `tofu validate`      |
| `terraform state`     | `tofu state`         |
| `terraform import`    | `tofu import`        |
| `terraform output`    | `tofu output`        |
| `terraform workspace` | `tofu workspace`     |

## Examples

### Example 1: AWS VPC Module

```hcl
# modules/vpc/main.tf
terraform {
  required_version = ">= 1.9.0"  # cross-object validation; raise for ephemeral/write-only
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

variable "name" {
  description = "Name prefix for resources"
  type        = string
}

variable "cidr_block" {
  description = "CIDR block for VPC"
  type        = string
  default     = "10.0.0.0/16"
}

variable "availability_zones" {
  description = "List of availability zones"
  type        = list(string)
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}

locals {
  common_tags = merge(var.tags, {
    Terraform   = "true"
    Module      = "vpc"
  })
}

resource "aws_vpc" "main" {
  cidr_block           = var.cidr_block
  enable_dns_hostnames = true
  enable_dns_support   = true

  tags = merge(local.common_tags, {
    Name = "${var.name}-vpc"
  })
}

resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id

  tags = merge(local.common_tags, {
    Name = "${var.name}-igw"
  })
}

# for_each keyed by AZ name -> stable addresses. Removing an AZ destroys only that
# subnet, never reindexing the survivors (which count would). The CIDR index is
# derived from a sorted-list lookup so it stays stable per AZ.
locals {
  az_index = { for i, az in sort(var.availability_zones) : az => i }
}

resource "aws_subnet" "public" {
  for_each = toset(var.availability_zones)

  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet(var.cidr_block, 4, local.az_index[each.key])
  availability_zone       = each.key
  map_public_ip_on_launch = true

  tags = merge(local.common_tags, {
    Name = "${var.name}-public-${each.key}"
    Tier = "public"
  })
}

resource "aws_subnet" "private" {
  for_each = toset(var.availability_zones)

  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.cidr_block, 4, local.az_index[each.key] + length(var.availability_zones))
  availability_zone = each.key

  tags = merge(local.common_tags, {
    Name = "${var.name}-private-${each.key}"
    Tier = "private"
  })
}

resource "aws_eip" "nat" {
  for_each = toset(var.availability_zones)
  domain   = "vpc"

  tags = merge(local.common_tags, {
    Name = "${var.name}-nat-eip-${each.key}"
  })
}

resource "aws_nat_gateway" "main" {
  for_each = toset(var.availability_zones)

  allocation_id = aws_eip.nat[each.key].id
  subnet_id     = aws_subnet.public[each.key].id

  tags = merge(local.common_tags, {
    Name = "${var.name}-nat-${each.key}"
  })

  depends_on = [aws_internet_gateway.main]
}

output "vpc_id" {
  description = "VPC ID"
  value       = aws_vpc.main.id
}

output "public_subnet_ids" {
  description = "Public subnet IDs"
  value       = [for s in aws_subnet.public : s.id]   # for_each map -> list
}

output "private_subnet_ids" {
  description = "Private subnet IDs"
  value       = [for s in aws_subnet.private : s.id]
}
```

### Example 2: EKS Cluster Configuration

```hcl
# main.tf
terraform {
  required_version = ">= 1.9.0"

  backend "s3" {
    bucket       = "my-terraform-state"
    key          = "eks/terraform.tfstate"
    region       = "us-west-2"
    encrypt      = true
    use_lockfile = true   # native S3 locking; replaces deprecated dynamodb_table
  }
}

provider "aws" {
  region = var.region

  default_tags {
    tags = {
      Environment = var.environment
      Project     = var.project
      ManagedBy   = "terraform"
    }
  }
}

module "vpc" {
  source = "./modules/vpc"

  name               = "${var.project}-${var.environment}"
  cidr_block         = var.vpc_cidr
  availability_zones = var.availability_zones
  tags               = var.tags
}

module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 19.0"

  cluster_name    = "${var.project}-${var.environment}"
  cluster_version = "1.28"

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnet_ids

  cluster_endpoint_public_access = true

  eks_managed_node_groups = {
    general = {
      desired_size = 2
      min_size     = 1
      max_size     = 5

      instance_types = ["t3.medium"]
      capacity_type  = "ON_DEMAND"

      labels = {
        role = "general"
      }
    }
  }

  tags = var.tags
}
```

### Example 3: Variables and Outputs

```hcl
# variables.tf
variable "region" {
  description = "AWS region"
  type        = string
  default     = "us-west-2"
}

variable "environment" {
  description = "Environment name"
  type        = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be dev, staging, or prod."
  }
}

variable "project" {
  description = "Project name"
  type        = string
}

variable "vpc_cidr" {
  description = "VPC CIDR block"
  type        = string
  default     = "10.0.0.0/16"
}

variable "availability_zones" {
  description = "List of availability zones"
  type        = list(string)
  default     = ["us-west-2a", "us-west-2b", "us-west-2c"]
}

variable "tags" {
  description = "Additional tags"
  type        = map(string)
  default     = {}
}

# outputs.tf
output "vpc_id" {
  description = "VPC ID"
  value       = module.vpc.vpc_id
}

output "eks_cluster_endpoint" {
  description = "EKS cluster endpoint"
  value       = module.eks.cluster_endpoint
  sensitive   = true
}

output "eks_cluster_name" {
  description = "EKS cluster name"
  value       = module.eks.cluster_name
}
```

### Example 4: Multi-Environment with Terraform Workspaces

```hcl
# main.tf
locals {
  environment = terraform.workspace

  # Environment-specific configuration
  config = {
    dev = {
      instance_type  = "t3.small"
      min_size       = 1
      max_size       = 3
      enable_backups = false
    }
    staging = {
      instance_type  = "t3.medium"
      min_size       = 2
      max_size       = 5
      enable_backups = true
    }
    prod = {
      instance_type  = "m5.large"
      min_size       = 3
      max_size       = 10
      enable_backups = true
    }
  }

  current_config = local.config[local.environment]
}

resource "aws_autoscaling_group" "app" {
  name             = "${var.project}-${local.environment}-asg"
  min_size         = local.current_config.min_size
  max_size         = local.current_config.max_size
  desired_capacity = local.current_config.min_size

  launch_template {
    id      = aws_launch_template.app.id
    version = "$Latest"
  }
}
```
