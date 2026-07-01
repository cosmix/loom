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

Terraform/OpenTofu infrastructure: module design, state/backends, multi-environment strategy, provider config, refactoring/import. **Prefer `tofu` over `terraform`** — OpenTofu is the Linux Foundation fork, config-compatible, and has capabilities OSS Terraform lacks (client-side state encryption). Command mapping table is at the end.

The single highest-value section is **Expert Practices** below — the anti-patterns/gotchas there are where naive-but-working configs go wrong.

## Module & Configuration Design

Canonical layout — shared `modules/`, one root module per environment, lock file committed:

```text
terraform/
├── modules/{vpc,eks,rds}/         # each: main.tf variables.tf outputs.tf versions.tf
├── environments/{dev,staging,prod}/  # each: main.tf backend.tf terraform.tfvars .terraform.lock.hcl
└── .terraform.lock.hcl
```

Principles: reusable modules with typed, validated, described inputs; providers pinned in `required_providers`; remote state with locking; `locals` for computed/DRY values; documented outputs; `lifecycle` rules where deletion/replacement risk exists.

**Variable validation** — validate at the boundary; `contains()` for enums, length/regex for shape:

```hcl
variable "environment" {
  type = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be dev, staging, or prod."
  }
}
```

**Module composition** — wire flat modules via input/output, never nest deep (see Anti-Patterns). Version external modules with `~>`; pin internal git modules by tag `ref=`:

```hcl
module "vpc" { source = "terraform-aws-modules/vpc/aws", version = "~> 5.0" }
module "internal" { source = "git::https://github.com/org/tf-modules.git//vpc?ref=v1.2.3" }
```

## State & Backends

**S3 with native locking** (no DynamoDB):

```hcl
terraform {
  backend "s3" {
    bucket       = "myorg-terraform-state"
    key          = "prod/eks/terraform.tfstate"
    region       = "us-west-2"
    encrypt      = true
    kms_key_id   = "arn:aws:kms:us-west-2:123456789012:key/..."
    use_lockfile = true   # native S3 conditional-write lock; no DynamoDB table/IAM
  }
}
```

- `use_lockfile = true` locks via an S3 conditional `PutObject` of `<key>.tflock`. In **Terraform**, `dynamodb_table` locking is deprecated (future removal); migrate by keeping BOTH until all CI/operators cut over, then drop `dynamodb_table`. **OpenTofu has NOT deprecated DynamoDB locking** — this is Terraform-only.
- **`backend` blocks cannot reference `var.*`/`local.*`/`data.*`** — evaluated before variables exist; any reference errors at `init`. Use *partial configuration*: omit dynamic args, supply via `-backend-config=KEY=VALUE` or a `.tfbackend` file.
- **NEVER pass credentials via `-backend-config`** (persisted into `.terraform/` and plan files) or hardcode them — supply backend creds only through env vars (`AWS_ACCESS_KEY_ID`, etc.).
- `lifecycle` is a resource meta-argument — **invalid inside a `backend` block**. Protect the state bucket via `prevent_destroy` on its own `aws_s3_bucket` resource + versioning/object-lock.

```bash
tofu init -backend-config="bucket=$TF_STATE_BUCKET" -backend-config="key=prod/app.tfstate"
```

**State operations** — prefer the version-controlled block form (`moved`/`removed`/`import`, see Idioms) over these imperative commands:

| Command | Purpose | Prefer |
| --- | --- | --- |
| `tofu state list` / `show <addr>` | inspect | — |
| `tofu state mv A B` | rename/move | `moved` block |
| `tofu state rm <addr>` | drop from state (no destroy) | `removed { lifecycle { destroy = false } }` |
| `tofu import <addr> <id>` | adopt existing resource | `import` block (+ `-generate-config-out`) |
| `tofu state pull > s.json` / `push s.json` | raw read/write (push is DANGEROUS) | — |
| `tofu state replace-provider OLD NEW` | after provider migration | — |
| `tofu force-unlock <LOCK_ID>` | clear stuck lock (verify nothing running first) | — |

**State migration** — configure new backend, then `tofu init -migrate-state` (add `-backend-config=...` for target). Always encrypt remote state; never commit state files.

## Multi-Environment

**Strategy 1 (RECOMMENDED): directory-per-env.** Each env is a separate root module with its OWN backend bucket and IAM role — switching envs requires a deliberate `cd`, so a dev shell physically cannot target prod. Right choice for dev/staging/prod.

**Strategy 2: workspaces — NOT environment isolation.** Official docs: workspaces "are not appropriate for system decomposition or deployments requiring separate credentials and access controls." All workspaces share ONE backend, ONE auth context, ONE provider config — no prod/dev boundary, and a mistyped `tofu workspace select prod && tofu apply` hits prod with no config guard. Use ONLY for short-lived variants within a single access boundary (PR-preview/ephemeral test envs).

**Strategy 3: tfvars files** — one root module, per-env var files. Flexible but shares backend/creds like workspaces unless combined with Strategy 1:

```bash
tofu apply -var-file="environments/prod.tfvars"
```

## Security

Compact, non-obvious flags (the surrounding IAM/SG HCL is standard):

```hcl
# KMS: rotation + generous deletion window
resource "aws_kms_key" "data" {
  enable_key_rotation     = true
  deletion_window_in_days = 30
}

# S3 SSE-KMS with bucket keys (cuts KMS API cost ~99% on high-volume buckets)
resource "aws_s3_bucket_server_side_encryption_configuration" "data" {
  bucket = aws_s3_bucket.data.id
  rule {
    apply_server_side_encryption_by_default { kms_master_key_id = aws_kms_key.data.arn, sse_algorithm = "aws:kms" }
    bucket_key_enabled = true
  }
}

# RDS encryption is create-time only — cannot be toggled on an existing instance
resource "aws_db_instance" "main" { storage_encrypted = true, kms_key_id = aws_kms_key.data.arn }
```

- **Define SG rules as separate `aws_security_group_rule` (or `aws_vpc_security_group_ingress_rule`) resources**, not inline `ingress`/`egress` blocks, when two SGs reference each other — inline blocks force a dependency cycle; separate rule resources break it.
- Store credentials in Secrets Manager/SSM and pass only ARNs; mark any secret-bearing output `sensitive = true` (but see Security below — that does NOT keep it out of state).
- IAM: scope `Action`/`Resource` to exact ARNs; avoid `"*"` outside the KMS root key policy.

## Providers

```hcl
terraform {
  # Floor = primitives this config uses. 1.5 import/check blocks; 1.9 cross-object validation.
  # Raise: >=1.10 ephemeral values; >=1.11 write-only args + native S3 locking.
  # OpenTofu: >=1.7 state encryption; >=1.10 S3 locking.
  required_version = ">= 1.9.0"
  required_providers {
    aws        = { source = "hashicorp/aws", version = "~> 5.0" }   # 5.x, not 6.0
    kubernetes = { source = "hashicorp/kubernetes", version = "~> 2.23" }
  }
}

provider "aws" {
  region       = var.region
  default_tags { tags = { Environment = var.environment, ManagedBy = "terraform" } }
}

# Aliases for multi-region/account — resources/modules select via `provider =` / `providers = {}`
provider "aws" { alias = "replica", region = "us-east-1" }
resource "aws_s3_bucket" "replica" { provider = aws.replica, bucket = "${var.name}-replica" }
```

`default_tags` auto-applies to every taggable resource — no per-resource `merge()` needed for org-wide tags.

## Core Patterns

**Dynamic blocks** — generate repeatable nested blocks from a collection:

```hcl
dynamic "ingress" {
  for_each = var.ingress_rules
  content {
    from_port = ingress.value.from_port
    to_port   = ingress.value.to_port
    protocol  = ingress.value.protocol
    cidr_blocks = ingress.value.cidr_blocks
  }
}
```

**`for` expressions** — build maps/lists; filter with `if`:

```hcl
subnet_by_az   = { for s in aws_subnet.private : s.availability_zone => s.id }
prod_instances = { for k, v in var.instances : k => v if v.environment == "production" }
```

`count` vs `for_each`, `lifecycle` (`create_before_destroy`/`prevent_destroy`/`ignore_changes`), and `depends_on` are covered in depth under **Expert Practices** — read the Gotchas there before using any of them.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| State lock timeout | Verify nothing running, then `tofu force-unlock <LOCK_ID>` |
| Corrupt provider cache | `rm -rf .terraform/ && tofu init` — do NOT delete `.terraform.lock.hcl` |
| Resource exists but not in state | `import` block (or `tofu import`), then reconcile HCL |
| Circular dependency | Extract the coupling into a standalone resource (e.g. `aws_security_group_rule`) or make deps unidirectional |
| Force replacement | `tofu apply -replace="aws_x.y"` — `taint` is DEPRECATED (mutates state with no plan preview) |
| Drift from console change | `tofu plan -refresh-only` to detect; `tofu apply` to restore code definition |
| Module source/version changed | `tofu init -upgrade` |
| Long-running create/update/delete | resource `timeouts { create = "60m" ... }` block |

**Regenerate (never just delete) a corrupt lock file** across all target platforms — deleting it discards validated checksum pins and can pull a different version next `init`:

```bash
tofu providers lock -platform=linux_amd64 -platform=darwin_arm64 -platform=windows_amd64
```

**Sensitive data in state:** `sensitive = true` only redacts UI output — the value is still plaintext in state. Real fixes: write-only args (TF ≥1.11) + ephemeral resources (≥1.10) never persist; OpenTofu ≥1.7 `encryption` block; or external store passing only ARNs. See Expert Practices → Security.

## Testing & Validation

```bash
tofu fmt -recursive      # format (run before every commit)
tofu validate            # config validity (no cloud calls)
tfsec .                  # or trivy/checkov — static security scan
infracost breakdown --path .   # cost estimate
```

**Plan review checklist:** all changes expected · no unintended `-/+` (destroy/replace) · sensitive values not in outputs · encryption enabled · IAM least-privilege · SGs restrictive · tags present.

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance from the official docs. The *why* matters as much as the rule.

### Anti-Patterns

**Never put a `provider` block in a reusable child module.** Docs: "A module intended to be called by one or more other modules must not contain any provider blocks," and "a module with its own provider configurations is not compatible with for_each, count, or depends_on" on the call. Mechanism: a provider config and the resources it manages must be destroyed together, but the graph cannot guarantee that ordering once the call is multiplied/ordered. Declare `provider` blocks only in root modules; pass aliased providers explicitly via `providers`. **Aliased providers are NEVER inherited** — forgetting to pass one silently falls back to the default (wrong region/account). Child modules declare `required_providers` for version constraints only — no provider block, no `backend` block.

```hcl
provider "aws" { alias = "usw2", region = "us-west-2" }
module "app" {
  source    = "./modules/app"
  providers = { aws = aws.usw2 }
  for_each  = var.environments   # works only because the module has no provider block
}
```

**Keep module composition flat (single level of child modules).** Modules take inputs, emit outputs, and know nothing about where state lives. Compose flat at the root and wire via input/output rather than nesting (root → A → B → C), which obscures dependency flow, complicates provider passing, and makes refactoring hazardous.

### Idioms

**Don't repeat the resource type in the name label.** `aws_security_group.app` already carries the type; `aws_security_group.app_security_group` says the noun twice. Short role-descriptive nouns, underscores not dashes (`aws_instance.web`, not `aws_instance.ec2-web-server`). The `this`/`main` singleton convention is a *community* idiom, NOT in the official style guide.

**Use `moved`/`removed`/`import` blocks, not the imperative CLI.** `state mv`/`import`/`taint` are imperative: every operator must run them, they aren't version-controlled or reproducible. Block forms are validated, reviewable in PRs, and run in CI with no manual steps.

- `moved` (≥1.1) encodes renames. **Never delete a published `moved` block** — it makes configs referencing the old address plan a *delete* instead of a move. Chain renames across successive moves; retain all historical blocks.
- `removed` (≥1.7) with `lifecycle { destroy = false }` drops a resource from management WITHOUT destroying real infra.
- `import` (≥1.5) blocks make import plannable; pair with `-generate-config-out=FILE` to auto-generate HCL for brownfield adoption. OpenTofu 1.7+ adds `for_each` for bulk imports. Unlike `moved`, import blocks may be deleted after they succeed.

```hcl
moved   { from = aws_instance.server,    to = aws_instance.app_server }     # chain, never prune
removed { from = aws_db_instance.legacy, lifecycle { destroy = false } }    # keep real infra
import  { to   = aws_instance.app,       id = "i-1234567890abcdef0" }        # then -generate-config-out
```

**Commit `.terraform.lock.hcl`, pre-populate multi-platform checksums.** `init` records checksums only for the platform it ran on, so a teammate/CI on another OS/arch hits "no matching checksum". The lock pins PROVIDERS only — module pinning lives in the `version` constraint. A CI `init -upgrade` re-resolves from constraints and overwrites lock selections, silently unpinning.

```bash
tofu providers lock -platform=linux_amd64 -platform=darwin_amd64 \
                    -platform=darwin_arm64 -platform=windows_amd64
```

### Gotchas

**`count` index shift silently recreates the WRONG resources — prefer `for_each`.** Removing a non-tail element of a `count` list reindexes everything after it: removing `[1]` of three makes old-`[2]` become `[1]`, so Terraform destroys & recreates the resource you never touched — a data-loss footgun for RDS/EBS/subnets. Key `for_each` over a map/set by STABLE identity. Caveats: `toset()` on a list silently dedupes duplicates (dropping instances), and a `moved` block migrates existing count-indexed state to for_each keys without recreation.

```hcl
# BAD — count: removing var.names[1] reindexes [2]→[1], recreating a survivor
resource "aws_instance" "app" { count = length(var.names), ami = var.ami }
# GOOD — for_each: address keyed by stable name, deletes only the removed key
resource "aws_instance" "app" { for_each = toset(var.names), ami = var.ami, tags = { Name = each.value } }
```

**`for_each` keys must be known at plan time and non-sensitive.** Keys appear in plan UI, so (1) sensitive values are categorically forbidden as `for_each` args, and (2) a computed/"known after apply" attribute (generated ID/ARN/endpoint) as a key is a plan-time error. Impure functions (`uuid()`, `timestamp()`, `bcrypt()`) are disallowed too. Derive a non-sensitive, statically-known key set with a `for` expression first.

**Data sources are read at plan time and return last-known values.** A `data` lookup reflects state as of the plan, not the live apply moment. If any argument (or an added `depends_on`) references a "known after apply" value, the read is DEFERRED to apply and the plan shows "(known after apply)", making plan review meaningless. Keep data-source args static; to use an attribute of a resource you're creating, **reference that resource's attribute directly** rather than re-looking it up via a data source.

**`create_before_destroy` is graph-wide and irreversible.** Terraform "propagates and applies create_before_destroy behavior to all resource dependencies" and stores it in state; you cannot override it to `false` on a dependency (that would imply a cycle). One leaf change can silently alter replacement ordering of upstream infra. Two more traps: with CBD true, `destroy`-time provisioners do NOT run (drain/deregister logic skipped); and unique-name resources (SGs, RDS, IAM, S3) collide during the create-then-destroy overlap — use `name_prefix` or a `random_id`/`random_pet` suffix.

```hcl
resource "aws_security_group" "app" {
  name_prefix = "${var.name}-app-"   # unique per replacement; avoids overlap collision
  lifecycle { create_before_destroy = true }
}
```

**`prevent_destroy` is bypassed when you delete the resource block.** It blocks destroy plans only WHILE the block exists; Terraform does not store the rule in state (unlike `create_before_destroy`). Deleting the block makes the next apply plan destruction with no guard. To stop managing a resource without destroying it, use a `removed` block with `destroy = false`.

### Performance

**Prefer implicit attribute references over `depends_on`.** Referencing an attribute (`aws_iam_instance_profile.app.name`) gives Terraform the exact dependency scope and full parallelism. `depends_on` is blunt: it plans conservatively, marks more values "(known after apply)", and "can cause Terraform to create more conservative plans that replace more resources than necessary." Worst on a `module` call — it serializes ALL resources inside. Reserve for hidden side effects with no referenceable attribute (e.g. an IAM policy that must propagate before bootstrap).

### Security

**`sensitive = true` does NOT protect state.** Docs: "Terraform stores values with the sensitive argument in both state and plan files, and anyone who can access those files can access your sensitive values." `terraform output -json`/`-raw` print them plaintext regardless. Only mechanisms that keep a secret out of state:

- **Write-only arguments** (TF ≥1.11, provider-specific, e.g. `password_wo` + `password_wo_version`) — provider consumes them, never persisted.
- **Ephemeral resources/values** (≥1.10) — fetched per-phase, discarded before state/plan are written.

For older versions, keep the secret in an external store and pass only ARNs/references.

```hcl
ephemeral "aws_secretsmanager_secret_version" "db" { secret_id = aws_secretsmanager_secret.db.id }
resource "aws_db_instance" "main" {
  password_wo         = ephemeral.aws_secretsmanager_secret_version.db.secret_string
  password_wo_version = 1   # increment to rotate
}
```

**Avoid `terraform_remote_state` across team boundaries.** It exposes only outputs, but the consumer needs read access to the ENTIRE state snapshot to get them — "any user or server which has enough access to read the root module output values will also always have access to the full state snapshot data," often including secrets. Renaming an output breaks every consumer. Publish explicitly to a neutral store (SSM Parameter Store, Consul, DNS) so access controls differ; HCP Terraform's `tfe_outputs` avoids full-state access. Within one team's own repo, `terraform_remote_state` is fine.

```hcl
resource "aws_ssm_parameter" "vpc_id" { name = "/shared/networking/vpc-id", type = "String", value = aws_vpc.main.id }
data "aws_ssm_parameter" "vpc_id"     { name = "/shared/networking/vpc-id" }   # consumer needs no state access
```

### Validation (Design Pattern)

Layered validation, each tier with a distinct scope and failure behavior:

- **Variable `validation`** — checks raw input shape/range. Since 1.9 it may reference other objects, but CANNOT reach a `data` source / provider-returned attribute. Halts.
- **`precondition`/`postcondition`** (≥1.2, inside `lifecycle`) — run with resolved values: preconditions assert cross-resource invariants before create; postconditions validate provider-returned attributes after create. Both HALT on failure.
- **`check` blocks** (≥1.5) — run at the END of plan/apply and report failures as WARNINGS without halting. Use for health probes, cert-expiry, compliance/drift that should surface but not gate a deploy; can embed a scoped `data` source.

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

**OpenTofu offers client-side state encryption; OSS Terraform does not.** OpenTofu 1.7+ adds an `encryption` block inside `terraform {}` encrypting state and plan (AES-GCM, PBKDF2 passphrase or KMS key provider) before they leave the process — protecting secrets at rest independent of backend encryption (S3 SSE still leaves state readable to anyone with bucket access). Tradeoff: tools that parse raw state (remote-state data sources, drift comparison) don't work against encrypted state — plan for key distribution. Use a KMS key provider (not a static passphrase) in production for rotation/audit.

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

## Verification Checklist

- [ ] `tofu fmt -recursive` clean and `tofu validate` passes
- [ ] `.terraform.lock.hcl` committed with all target platforms locked (`tofu providers lock -platform=...`)
- [ ] Provider versions pinned with `~>`; `required_version` floor matches primitives used
- [ ] No `provider`/`backend` block inside any child module; aliased providers passed explicitly
- [ ] Resource collections use `for_each` over stable keys (not `count`) where mid-list removal is possible
- [ ] Renames/removals/imports done via `moved`/`removed`/`import` blocks, not imperative CLI
- [ ] Remote state encrypted + locked; no state files committed; no creds in `-backend-config`
- [ ] Secrets kept out of state (write-only args / ephemeral / external store) — not just `sensitive = true`
- [ ] `plan` reviewed: no unexpected destroy/replace; `prevent_destroy` on stateful resources
- [ ] Env isolation via directory-per-env (workspaces only for ephemeral same-boundary variants)

## OpenTofu vs Terraform Commands

`tofu` is preferred; substitute `terraform` where OpenTofu is unavailable. Same subcommands throughout (`init`, `plan`, `apply`, `destroy`, `fmt`, `validate`, `state`, `import`, `output`, `workspace`).

## Example: AWS VPC Module (for_each idiom)

Demonstrates the stable-address pattern — subnets keyed by AZ so removing one AZ destroys only its subnet, never reindexing survivors (which `count` would):

```hcl
terraform {
  required_version = ">= 1.9.0"
  required_providers { aws = { source = "hashicorp/aws", version = "~> 5.0" } }
}

variable "name"       { type = string }
variable "cidr_block" { type = string, default = "10.0.0.0/16" }
variable "availability_zones" { type = list(string) }

resource "aws_vpc" "main" {
  cidr_block           = var.cidr_block
  enable_dns_hostnames = true
  enable_dns_support   = true
  tags                 = { Name = "${var.name}-vpc" }
}

# CIDR index derived from a sorted-list lookup so it stays stable per AZ
locals { az_index = { for i, az in sort(var.availability_zones) : az => i } }

resource "aws_subnet" "public" {
  for_each                = toset(var.availability_zones)
  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet(var.cidr_block, 4, local.az_index[each.key])
  availability_zone       = each.key
  map_public_ip_on_launch = true
  tags                    = { Name = "${var.name}-public-${each.key}", Tier = "public" }
}

resource "aws_subnet" "private" {
  for_each          = toset(var.availability_zones)
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.cidr_block, 4, local.az_index[each.key] + length(var.availability_zones))
  availability_zone = each.key
  tags              = { Name = "${var.name}-private-${each.key}", Tier = "private" }
}

output "private_subnet_ids" {
  value = [for s in aws_subnet.private : s.id]   # for_each map -> list
}
```

Root module wiring (backend + external module + local module):

```hcl
terraform {
  required_version = ">= 1.9.0"
  backend "s3" {
    bucket = "my-terraform-state", key = "eks/terraform.tfstate"
    region = "us-west-2", encrypt = true, use_lockfile = true
  }
}
module "vpc" { source = "./modules/vpc", name = var.name, availability_zones = var.azs }
module "eks" {
  source          = "terraform-aws-modules/eks/aws"
  version         = "~> 19.0"
  cluster_name    = var.name
  cluster_version = "1.28"
  vpc_id          = module.vpc.vpc_id
  subnet_ids      = module.vpc.private_subnet_ids
}
```
