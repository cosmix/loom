---
name: terraform
description: Creates and manages Infrastructure as Code using Terraform/OpenTofu for cloud resource provisioning, module development, state management, and multi-environment deployments. Trigger keywords: terraform, tf, opentofu, tofu, infrastructure, IaC, infrastructure as code, provision, cloud, aws, azure, gcp, kubernetes, k8s, module, provider, state, backend, plan, apply, workspace, resource, data source, output, variable, locals, import, taint, destroy.
allowed-tools: Read, Grep, Glob, Edit, Write, Bash
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

**Primary User: senior-infrastructure-engineer** - This skill supports the senior-infrastructure-engineer agent for all Terraform architecture and implementation work.

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
```
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

**AWS S3 + DynamoDB:**
```hcl
# backend.tf
terraform {
  backend "s3" {
    bucket         = "myorg-terraform-state"
    key            = "prod/eks/terraform.tfstate"
    region         = "us-west-2"
    encrypt        = true
    kms_key_id     = "arn:aws:kms:us-west-2:123456789012:key/..."
    dynamodb_table = "terraform-state-locks"

    # Prevent accidental deletion
    lifecycle {
      prevent_destroy = true
    }
  }
}
```

**Setup Commands:**
```bash
# Create state bucket and lock table
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

aws dynamodb create-table \
  --table-name terraform-state-locks \
  --attribute-definitions AttributeName=LockID,AttributeType=S \
  --key-schema AttributeName=LockID,KeyType=HASH \
  --billing-mode PAY_PER_REQUEST
```

**State Operations:**
```bash
# List resources in state
tofu state list

# Show specific resource
tofu state show aws_vpc.main

# Move resource to different address
tofu state mv aws_instance.old aws_instance.new

# Remove resource from state (doesn't destroy)
tofu state rm aws_instance.temp

# Import existing resource
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
- DynamoDB for AWS, GCS for Google Cloud, Azure Storage for Azure
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

**Strategy 1: Workspaces (Simple, Same Backend)**
```bash
# Create and switch workspaces
tofu workspace new dev
tofu workspace new staging
tofu workspace new prod

tofu workspace list
tofu workspace select prod

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

**Strategy 2: Directory Structure (Complex, Isolated)**
```
terraform/
├── modules/          # Shared modules
├── environments/
│   ├── dev/
│   │   ├── main.tf
│   │   ├── backend.tf
│   │   ├── terraform.tfvars
│   │   └── .terraform.lock.hcl
│   ├── staging/
│   └── prod/
```

**Strategy 3: tfvars Files (Flexible)**
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
  required_version = ">= 1.6.0"

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

**Issue: State Lock Timeout**
```bash
# Problem: Another process has state locked
# Solution 1: Wait for other process to complete
# Solution 2: Verify no process running, then force unlock
tofu force-unlock <LOCK_ID>
```

**Issue: Provider Plugin Errors**
```bash
# Problem: Corrupted provider cache
# Solution: Clear cache and re-initialize
rm -rf .terraform/
rm .terraform.lock.hcl
tofu init
```

**Issue: Resource Already Exists**
```bash
# Problem: Resource exists but not in state
# Solution: Import existing resource
tofu import aws_instance.example i-1234567890abcdef0

# Or: Remove from code and manage outside Terraform
```

**Issue: Circular Dependency**
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

**Issue: Sensitive Data in State**
```bash
# Problem: Passwords/keys visible in state file
# Solution: Always use remote state with encryption
# Never commit state files to version control
# Use AWS Secrets Manager / Vault for secrets
```

**Issue: Resource Drift**
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

**Issue: Module Source Changes**
```bash
# Problem: Module source or version changed
# Solution: Re-initialize and upgrade
tofu init -upgrade

# Lock provider versions
tofu providers lock
```

**Issue: Large Plan Output**
```bash
# Filter plan output
tofu plan | grep "will be created"

# Save plan for review
tofu plan -out=tfplan
tofu show tfplan

# Show only specific resource types
tofu plan | grep "aws_instance"
```

**Issue: Timeout Errors**
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

| Terraform            | OpenTofu (Preferred) |
| -------------------- | -------------------- |
| `terraform init`     | `tofu init`          |
| `terraform plan`     | `tofu plan`          |
| `terraform apply`    | `tofu apply`         |
| `terraform destroy`  | `tofu destroy`       |
| `terraform fmt`      | `tofu fmt`           |
| `terraform validate` | `tofu validate`      |
| `terraform state`    | `tofu state`         |
| `terraform import`   | `tofu import`        |
| `terraform output`   | `tofu output`        |
| `terraform workspace`| `tofu workspace`     |

## Examples

### Example 1: AWS VPC Module

```hcl
# modules/vpc/main.tf
terraform {
  required_version = ">= 1.0"
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

resource "aws_subnet" "public" {
  count = length(var.availability_zones)

  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet(var.cidr_block, 4, count.index)
  availability_zone       = var.availability_zones[count.index]
  map_public_ip_on_launch = true

  tags = merge(local.common_tags, {
    Name = "${var.name}-public-${var.availability_zones[count.index]}"
    Tier = "public"
  })
}

resource "aws_subnet" "private" {
  count = length(var.availability_zones)

  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.cidr_block, 4, count.index + length(var.availability_zones))
  availability_zone = var.availability_zones[count.index]

  tags = merge(local.common_tags, {
    Name = "${var.name}-private-${var.availability_zones[count.index]}"
    Tier = "private"
  })
}

resource "aws_eip" "nat" {
  count  = length(var.availability_zones)
  domain = "vpc"

  tags = merge(local.common_tags, {
    Name = "${var.name}-nat-eip-${count.index}"
  })
}

resource "aws_nat_gateway" "main" {
  count = length(var.availability_zones)

  allocation_id = aws_eip.nat[count.index].id
  subnet_id     = aws_subnet.public[count.index].id

  tags = merge(local.common_tags, {
    Name = "${var.name}-nat-${count.index}"
  })

  depends_on = [aws_internet_gateway.main]
}

output "vpc_id" {
  description = "VPC ID"
  value       = aws_vpc.main.id
}

output "public_subnet_ids" {
  description = "Public subnet IDs"
  value       = aws_subnet.public[*].id
}

output "private_subnet_ids" {
  description = "Private subnet IDs"
  value       = aws_subnet.private[*].id
}
```

### Example 2: EKS Cluster Configuration

```hcl
# main.tf
terraform {
  required_version = ">= 1.0"

  backend "s3" {
    bucket         = "my-terraform-state"
    key            = "eks/terraform.tfstate"
    region         = "us-west-2"
    encrypt        = true
    dynamodb_table = "terraform-locks"
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
