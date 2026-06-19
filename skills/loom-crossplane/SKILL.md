---
name: loom-crossplane
description: Cloud-native infrastructure management with Crossplane via Kubernetes APIs. Use for building internal platform APIs, composite resources, XRDs, compositions, claims, provider configuration, and multi-cloud self-service provisioning.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - crossplane
  - XRD
  - composition
  - claim
  - provider
  - managed resource
  - composite resource
  - infrastructure API
  - platform engineering
  - platform API
  - infrastructure abstraction
  - self-service infrastructure
  - kubernetes infrastructure
  - cloud control plane
---

# Crossplane Infrastructure Management

Crossplane extends Kubernetes to manage cloud infrastructure using declarative APIs. It enables platform teams to build internal cloud platforms with self-service capabilities.

## Architecture Overview

### Core Components

1. **Providers**: Kubernetes controllers that provision infrastructure in external systems (AWS, GCP, Azure, etc.)
2. **Managed Resources (MRs)**: Custom resources representing external infrastructure (S3 buckets, RDS instances, etc.)
3. **Composite Resources (XRs)**: Higher-level abstractions composed of multiple managed resources
4. **Composite Resource Definitions (XRDs)**: Schemas defining composite resource types
5. **Compositions**: Templates that map XRs to managed resources with transformation logic
6. **Claims**: Namespace-scoped resources that provision composite resources for application teams
7. **Composition Functions**: Extension points for complex transformation logic

### Resource Hierarchy

```text
Claim (namespace-scoped) -> Composite Resource (cluster-scoped) -> Managed Resources -> Cloud Infrastructure
```

## Installation and Setup

### Install Crossplane

```bash
# Add Crossplane Helm repository
helm repo add crossplane-stable https://charts.crossplane.io/stable
helm repo update

# Install Crossplane
# Composition functions are enabled by default since v1.14 — no feature-gate flag needed.
helm install crossplane \
  crossplane-stable/crossplane \
  --namespace crossplane-system \
  --create-namespace \
  --wait

# Verify installation
kubectl get pods -n crossplane-system
```

### Install Crossplane CLI

```bash
# Install CLI for local development
curl -sL https://raw.githubusercontent.com/crossplane/crossplane/master/install.sh | sh
sudo mv crossplane /usr/local/bin/

# Verify CLI
crossplane --version
```

## Provider Configuration

### AWS Provider

> **v2 note:** `ControllerConfig` (`pkg.crossplane.io/v1alpha1`) was deprecated in v1.11 and **removed in Crossplane v2**. Configure provider runtime with `DeploymentRuntimeConfig` (`pkg.crossplane.io/v1beta1`, beta-enabled by default since v1.14) referenced via `runtimeConfigRef`. Migrate any existing config with `crossplane beta convert deployment-runtime controller-config.yaml -o deployment-runtime-config.yaml` before upgrading.

```yaml
# providers/aws-provider.yaml
apiVersion: pkg.crossplane.io/v1
kind: Provider
metadata:
  name: provider-aws-s3
spec:
  package: xpkg.upbound.io/upbound/provider-aws-s3:v1.1.0
  runtimeConfigRef:
    name: aws-runtime
---
apiVersion: pkg.crossplane.io/v1
kind: Provider
metadata:
  name: provider-aws-rds
spec:
  package: xpkg.upbound.io/upbound/provider-aws-rds:v1.1.0
  runtimeConfigRef:
    name: aws-runtime
---
apiVersion: pkg.crossplane.io/v1beta1
kind: DeploymentRuntimeConfig
metadata:
  name: aws-runtime
spec:
  deploymentTemplate:
    spec:
      selector: {}
      template:
        spec:
          securityContext:
            fsGroup: 2000
          containers:
            - name: package-runtime
              args:
                - --poll=1m
                - --max-reconcile-rate=100
```

### Provider Authentication

```bash
# Create AWS credentials secret
kubectl create secret generic aws-creds \
  -n crossplane-system \
  --from-file=creds=/path/to/aws-credentials.txt

# credentials.txt format:
# [default]
# aws_access_key_id = AKIAIOSFODNN7EXAMPLE
# aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
```

```yaml
# providers/aws-provider-config.yaml
apiVersion: aws.upbound.io/v1beta1
kind: ProviderConfig
metadata:
  name: default
spec:
  credentials:
    source: Secret
    secretRef:
      namespace: crossplane-system
      name: aws-creds
      key: creds
```

### GCP Provider

```yaml
# providers/gcp-provider.yaml
apiVersion: pkg.crossplane.io/v1
kind: Provider
metadata:
  name: provider-gcp-storage
spec:
  package: xpkg.upbound.io/upbound/provider-gcp-storage:v1.1.0
---
apiVersion: gcp.upbound.io/v1beta1
kind: ProviderConfig
metadata:
  name: default
spec:
  projectID: my-gcp-project
  credentials:
    source: Secret
    secretRef:
      namespace: crossplane-system
      name: gcp-creds
      key: creds.json
```

## Managed Resources

### Direct Managed Resource Usage

```yaml
# managed-resources/s3-bucket.yaml
apiVersion: s3.aws.upbound.io/v1beta1
kind: Bucket
metadata:
  name: my-app-data-bucket
spec:
  forProvider:
    region: us-west-2
    tags:
      Environment: production
      ManagedBy: crossplane
  providerConfigRef:
    name: default
  deletionPolicy: Delete
```

```yaml
# managed-resources/rds-instance.yaml
apiVersion: rds.aws.upbound.io/v1beta1
kind: Instance
metadata:
  name: my-postgres-db
spec:
  forProvider:
    region: us-west-2
    allocatedStorage: 20
    engine: postgres
    engineVersion: "14.7"
    instanceClass: db.t3.micro
    dbName: myappdb
    username: dbadmin
    passwordSecretRef:
      namespace: crossplane-system
      name: db-password
      key: password
    skipFinalSnapshot: true
    publiclyAccessible: false
    vpcSecurityGroupIdSelector:
      matchLabels:
        role: database
  providerConfigRef:
    name: default
  writeConnectionSecretToRef:
    namespace: production
    name: postgres-connection
```

## Composite Resource Definitions (XRDs)

### Database XRD

> **v1 vs v2:** This XRD uses `apiextensions.crossplane.io/v1` with `claimNames` — the legacy cluster-scoped model. New XRDs should use `apiextensions.crossplane.io/v2`, which is **namespaced by default** (`scope: Namespaced`) and **drops claims** (no `claimNames`). `connectionSecretKeys` is a claims-era field that **does not apply to v2 XRDs**; on v2 aggregate connection details via `function-patch-and-transform`'s `writeConnectionSecretToRef.patches`. `spec.group` and `spec.names` are **immutable** — choose them carefully, since changing them requires deleting and recreating the XRD (which cascades to all its XRs).

```yaml
# xrds/database-xrd.yaml
apiVersion: apiextensions.crossplane.io/v1
kind: CompositeResourceDefinition
metadata:
  name: xpostgresqlinstances.database.example.com
spec:
  group: database.example.com
  names:
    kind: XPostgreSQLInstance
    plural: xpostgresqlinstances
  claimNames:
    kind: PostgreSQLInstance
    plural: postgresqlinstances
  connectionSecretKeys:
    - username
    - password
    - endpoint
    - port
  versions:
    - name: v1alpha1
      served: true
      referenceable: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                parameters:
                  type: object
                  properties:
                    storageGB:
                      type: integer
                      description: Size of the database in GB
                      default: 20
                      minimum: 20
                      maximum: 1000
                    size:
                      type: string
                      description: Instance size (small, medium, large)
                      enum: [small, medium, large]
                      default: small
                    engineVersion:
                      type: string
                      description: PostgreSQL version
                      default: "14.7"
                    highAvailability:
                      type: boolean
                      description: Enable multi-AZ deployment
                      default: false
                    backupRetentionDays:
                      type: integer
                      description: Number of days to retain backups
                      default: 7
                      minimum: 1
                      maximum: 35
                    networkRef:
                      type: object
                      description: Reference to network configuration
                      properties:
                        id:
                          type: string
                          description: Network identifier
                      required:
                        - id
                  required:
                    - size
                    - networkRef
              required:
                - parameters
            status:
              type: object
              properties:
                address:
                  type: string
                  description: Database endpoint address
```

### Application Platform XRD

```yaml
# xrds/app-platform-xrd.yaml
apiVersion: apiextensions.crossplane.io/v1
kind: CompositeResourceDefinition
metadata:
  name: xappplatforms.platform.example.com
spec:
  group: platform.example.com
  names:
    kind: XAppPlatform
    plural: xappplatforms
  claimNames:
    kind: AppPlatform
    plural: appplatforms
  connectionSecretKeys:
    - bucket_name
    - database_endpoint
    - database_password
    - cache_endpoint
  versions:
    - name: v1alpha1
      served: true
      referenceable: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                parameters:
                  type: object
                  properties:
                    environment:
                      type: string
                      description: Environment (dev, staging, prod)
                      enum: [dev, staging, prod]
                    appName:
                      type: string
                      description: Application name
                      pattern: "^[a-z0-9-]+$"
                    region:
                      type: string
                      description: AWS region
                      default: us-west-2
                    databaseSize:
                      type: string
                      description: Database instance size
                      enum: [small, medium, large]
                      default: small
                    enableCache:
                      type: boolean
                      description: Enable Redis cache
                      default: false
                  required:
                    - environment
                    - appName
              required:
                - parameters
```

## Compositions

### Database Composition with Size Mapping

> **v2 note — this is the v1-only `mode: Resources` layout, removed in Crossplane v2.** Native patch-and-transform (`spec.resources`/`spec.patchSets`) was deprecated in v1.17 and removed in v2; the only supported mode is `mode: Pipeline` with `function-patch-and-transform` (see the Pipeline-mode example under "Composition Functions" and the migration command `crossplane beta convert pipeline-composition old.yaml -o new.yaml`). Also, Composition-level `writeConnectionSecretsToNamespace` no longer functions for XRs in v2 (native XR connection details were removed) — aggregate via the function's `writeConnectionSecretToRef.patches` instead. To run on v2, move `resources`/`patchSets` verbatim into the `function-patch-and-transform` input.

```yaml
# compositions/postgres-composition.yaml  (v1-only mode: Resources — see v2 note above)
apiVersion: apiextensions.crossplane.io/v1
kind: Composition
metadata:
  name: postgres.aws.database.example.com
  labels:
    provider: aws
    database: postgresql
spec:
  writeConnectionSecretsToNamespace: crossplane-system
  compositeTypeRef:
    apiVersion: database.example.com/v1alpha1
    kind: XPostgreSQLInstance

  resources:
    # Security Group for Database
    - name: database-sg
      base:
        apiVersion: ec2.aws.upbound.io/v1beta1
        kind: SecurityGroup
        spec:
          forProvider:
            description: Security group for PostgreSQL database
            tags:
              Name: database-sg
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.networkRef.id
          toFieldPath: spec.forProvider.vpcId
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.labels[crossplane.io/claim-namespace]
          toFieldPath: spec.forProvider.tags.namespace
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.labels[crossplane.io/claim-name]
          toFieldPath: spec.forProvider.tags.claim

    # Security Group Rule - Postgres Port
    - name: database-sg-rule
      base:
        apiVersion: ec2.aws.upbound.io/v1beta1
        kind: SecurityGroupRule
        spec:
          forProvider:
            type: ingress
            fromPort: 5432
            toPort: 5432
            protocol: tcp
            cidrBlocks:
              - 10.0.0.0/8
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.networkRef.id
          toFieldPath: spec.forProvider.vpcId
        - type: PatchSet
          patchSetName: security-group-id

    # RDS Subnet Group
    - name: subnet-group
      base:
        apiVersion: rds.aws.upbound.io/v1beta1
        kind: SubnetGroup
        spec:
          forProvider:
            description: Subnet group for database
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.networkRef.id
          toFieldPath: metadata.labels[network-id]
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.uid
          toFieldPath: spec.forProvider.subnetIdSelector.matchLabels[subnet-group-id]

    # RDS Instance
    - name: rds-instance
      base:
        apiVersion: rds.aws.upbound.io/v1beta1
        kind: Instance
        spec:
          forProvider:
            engine: postgres
            skipFinalSnapshot: true
            publiclyAccessible: false
            username: dbadmin
            passwordSecretRef:
              namespace: crossplane-system
              key: password
            dbSubnetGroupNameSelector:
              matchControllerRef: true
            vpcSecurityGroupIdSelector:
              matchControllerRef: true
          writeConnectionSecretToRef:
            namespace: crossplane-system
      patches:
        # Instance size mapping
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.size
          toFieldPath: spec.forProvider.instanceClass
          transforms:
            - type: map
              map:
                small: db.t3.micro
                medium: db.t3.medium
                large: db.m5.large

        # Storage configuration
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.storageGB
          toFieldPath: spec.forProvider.allocatedStorage

        # Engine version
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.engineVersion
          toFieldPath: spec.forProvider.engineVersion

        # High availability
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.highAvailability
          toFieldPath: spec.forProvider.multiAz

        # Backup retention
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.backupRetentionDays
          toFieldPath: spec.forProvider.backupRetentionPeriod

        # Generate unique password secret name
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.uid
          toFieldPath: spec.forProvider.passwordSecretRef.name
          transforms:
            - type: string
              string:
                fmt: "%s-password"

        # Connection secret name
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.uid
          toFieldPath: spec.writeConnectionSecretToRef.name
          transforms:
            - type: string
              string:
                fmt: "%s-connection"

        # Expose endpoint to status
        - type: ToCompositeFieldPath
          fromFieldPath: status.atProvider.endpoint
          toFieldPath: status.address

        # Copy connection secret to claim namespace
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.labels[crossplane.io/claim-namespace]
          toFieldPath: spec.writeConnectionSecretToRef.namespace
          policy:
            fromFieldPath: Optional

  patchSets:
    - name: security-group-id
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: metadata.labels[security-group-id]
          toFieldPath: spec.forProvider.securityGroupIdSelector.matchLabels[security-group-id]
```

### Multi-Resource Application Platform Composition

> **v2 note — v1-only `mode: Resources` layout, removed in Crossplane v2** (same as above). Migrate to `mode: Pipeline` + `function-patch-and-transform` and replace `writeConnectionSecretsToNamespace` with the function's `writeConnectionSecretToRef.patches`.

```yaml
# compositions/app-platform-composition.yaml  (v1-only mode: Resources — see v2 note above)
apiVersion: apiextensions.crossplane.io/v1
kind: Composition
metadata:
  name: appplatform.aws.platform.example.com
  labels:
    provider: aws
spec:
  writeConnectionSecretsToNamespace: crossplane-system
  compositeTypeRef:
    apiVersion: platform.example.com/v1alpha1
    kind: XAppPlatform

  resources:
    # S3 Bucket for application data
    - name: storage-bucket
      base:
        apiVersion: s3.aws.upbound.io/v1beta1
        kind: Bucket
        spec:
          forProvider:
            tags:
              ManagedBy: crossplane
          deletionPolicy: Delete
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.region
          toFieldPath: spec.forProvider.region
        - type: CombineFromComposite
          combine:
            variables:
              - fromFieldPath: spec.parameters.appName
              - fromFieldPath: spec.parameters.environment
            strategy: string
            string:
              fmt: "%s-%s-data"
          toFieldPath: metadata.name
        - type: ToCompositeFieldPath
          fromFieldPath: metadata.name
          toFieldPath: status.bucketName
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.environment
          toFieldPath: spec.forProvider.tags.Environment

    # S3 Bucket versioning
    - name: bucket-versioning
      base:
        apiVersion: s3.aws.upbound.io/v1beta1
        kind: BucketVersioning
        spec:
          forProvider:
            versioningConfiguration:
              status: Enabled
            bucketSelector:
              matchControllerRef: true
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.region
          toFieldPath: spec.forProvider.region

    # S3 Bucket encryption
    - name: bucket-encryption
      base:
        apiVersion: s3.aws.upbound.io/v1beta1
        kind: BucketServerSideEncryptionConfiguration
        spec:
          forProvider:
            rule:
              applyServerSideEncryptionByDefault:
                sseAlgorithm: AES256
            bucketSelector:
              matchControllerRef: true
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.region
          toFieldPath: spec.forProvider.region

    # PostgreSQL Database (using our XRD)
    - name: database
      base:
        apiVersion: database.example.com/v1alpha1
        kind: XPostgreSQLInstance
        spec:
          parameters:
            engineVersion: "14.7"
            storageGB: 20
            highAvailability: false
            backupRetentionDays: 7
            networkRef:
              id: vpc-12345
          compositionSelector:
            matchLabels:
              provider: aws
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.databaseSize
          toFieldPath: spec.parameters.size
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.environment
          toFieldPath: spec.parameters.highAvailability
          transforms:
            - type: map
              map:
                dev: false
                staging: false
                prod: true
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.environment
          toFieldPath: spec.parameters.backupRetentionDays
          transforms:
            - type: map
              map:
                dev: 1
                staging: 7
                prod: 30
        - type: ToCompositeFieldPath
          fromFieldPath: status.address
          toFieldPath: status.databaseEndpoint

    # ElastiCache Redis
    # Note: For truly conditional resources, use Composition Functions with
    # function-conditional or create separate compositions. This example
    # always provisions the cache when included in the composition.
    - name: cache
      base:
        apiVersion: elasticache.aws.upbound.io/v1beta1
        kind: ReplicationGroup
        spec:
          forProvider:
            description: Redis cache cluster
            engine: redis
            engineVersion: "7.0"
            nodeType: cache.t3.micro
            numCacheClusters: 1
            automaticFailoverEnabled: false
            atRestEncryptionEnabled: true
            transitEncryptionEnabled: true
            securityGroupIdSelector:
              matchControllerRef: true
            subnetGroupNameSelector:
              matchControllerRef: true
      patches:
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.region
          toFieldPath: spec.forProvider.region
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.environment
          toFieldPath: spec.forProvider.automaticFailoverEnabled
          transforms:
            - type: map
              map:
                dev: false
                staging: false
                prod: true
        - type: FromCompositeFieldPath
          fromFieldPath: spec.parameters.environment
          toFieldPath: spec.forProvider.numCacheClusters
          transforms:
            - type: map
              map:
                dev: 1
                staging: 2
                prod: 3
        - type: ToCompositeFieldPath
          fromFieldPath: status.atProvider.primaryEndpointAddress
          toFieldPath: status.cacheEndpoint
```

## Claims (Self-Service Resources)

### Database Claim

```yaml
# claims/my-app-database.yaml
apiVersion: database.example.com/v1alpha1
kind: PostgreSQLInstance
metadata:
  name: my-app-db
  namespace: production
spec:
  parameters:
    size: medium
    storageGB: 100
    engineVersion: "14.7"
    highAvailability: true
    backupRetentionDays: 30
    networkRef:
      id: vpc-0a1b2c3d4e5f6g7h8
  compositionSelector:
    matchLabels:
      provider: aws
      database: postgresql
  writeConnectionSecretToRef:
    name: my-app-db-connection
```

### Application Platform Claim

```yaml
# claims/my-app-platform.yaml
apiVersion: platform.example.com/v1alpha1
kind: AppPlatform
metadata:
  name: my-application
  namespace: team-alpha
spec:
  parameters:
    environment: prod
    appName: my-app
    region: us-west-2
    databaseSize: large
    enableCache: true
  compositionSelector:
    matchLabels:
      provider: aws
  writeConnectionSecretToRef:
    name: my-app-platform-secrets
```

## Composition Functions

Composition Functions enable complex transformation logic using WebAssembly or container-based functions.

### Function Configuration

```yaml
# compositions/postgres-with-functions.yaml
apiVersion: apiextensions.crossplane.io/v1
kind: Composition
metadata:
  name: postgres.function-based.aws.database.example.com
spec:
  compositeTypeRef:
    apiVersion: database.example.com/v1alpha1
    kind: XPostgreSQLInstance

  mode: Pipeline
  pipeline:
    # Step 1: Patch and transform resources
    # Note: function-auto-ready only marks resources Ready — it does NOT generate
    # passwords or accept a Resources input. For secret/password generation use
    # function-kcl (random string) or External Secrets Operator; never write a
    # literal placeholder into a Secret.
    - step: patch-and-transform
      functionRef:
        name: function-patch-and-transform
      input:
        apiVersion: pt.fn.crossplane.io/v1beta1
        kind: Resources
        patchSets:
          - name: common-tags
            patches:
              - type: FromCompositeFieldPath
                fromFieldPath: metadata.labels[crossplane.io/claim-name]
                toFieldPath: spec.forProvider.tags.ClaimName
              - type: FromCompositeFieldPath
                fromFieldPath: metadata.labels[crossplane.io/claim-namespace]
                toFieldPath: spec.forProvider.tags.ClaimNamespace

        resources:
          - name: rds-instance
            base:
              apiVersion: rds.aws.upbound.io/v1beta1
              kind: Instance
              spec:
                forProvider:
                  engine: postgres
                  username: dbadmin
                  skipFinalSnapshot: true
            patches:
              - type: PatchSet
                patchSetName: common-tags
              - type: FromCompositeFieldPath
                fromFieldPath: spec.parameters.size
                toFieldPath: spec.forProvider.instanceClass
                transforms:
                  - type: map
                    map:
                      small: db.t3.micro
                      medium: db.t3.medium
                      large: db.m5.large

    # Step 2: Mark as ready
    - step: auto-ready
      functionRef:
        name: function-auto-ready
```

### Installing Composition Functions

# Community functions live on the neutral registry xpkg.crossplane.io (the default
# for crossplane-contrib since v1.20). Upbound's own providers stay on xpkg.upbound.io.
# Crossplane v2 has no default registry — always use a fully qualified URL.

```bash
# Install function-patch-and-transform
kubectl apply -f - <<EOF
apiVersion: pkg.crossplane.io/v1
kind: Function
metadata:
  name: function-patch-and-transform
spec:
  package: xpkg.crossplane.io/crossplane-contrib/function-patch-and-transform:v0.8.2
EOF

# Install function-auto-ready
kubectl apply -f - <<EOF
apiVersion: pkg.crossplane.io/v1
kind: Function
metadata:
  name: function-auto-ready
spec:
  package: xpkg.crossplane.io/crossplane-contrib/function-auto-ready:v0.4.1
EOF
```

## Best Practices

### Composition Patterns

#### Layered Abstraction Strategy

Build compositions in layers of increasing abstraction:

1. **Foundation Layer**: Provider-specific managed resources (S3, RDS, GCS)
2. **Resource Layer**: Cloud-agnostic XRDs (Database, ObjectStorage)
3. **Platform Layer**: Application-focused XRDs (AppPlatform, DataPlatform)

This enables teams to consume infrastructure at the right abstraction level.

#### Parallel Resource Creation

Crossplane automatically provisions resources in parallel when no dependencies exist. Optimize for this:

- Use selectors (matchControllerRef, matchLabels) instead of explicit refs
- Let Crossplane infer dependencies from resource relationships
- Avoid artificial ordering constraints

#### Transform Patterns

Common transform patterns for patches:

- **Size mapping**: map small/medium/large to instance types
- **Environment logic**: map dev/staging/prod to different configurations
- **String formatting**: CombineFromComposite with fmt for naming
- **Math operations**: multiply storage size by environment factor
- **Conditional values**: map boolean flags to provider-specific settings

#### Secret Aggregation

Merge connection secrets from multiple managed resources:

- Each managed resource writes to a unique secret
- Use connectionSecretKeys in XRD to define merged fields
- Crossplane automatically aggregates into composite secret
- Copy to claim namespace for application consumption

### Claim Design Patterns

#### Namespace Strategy

Choose a namespace model that fits your organization:

- **Per-team namespaces**: team-alpha, team-beta (good for multi-tenancy)
- **Per-environment namespaces**: dev, staging, prod (good for env isolation)
- **Hybrid approach**: team-alpha-prod, team-alpha-dev (maximum isolation)

Use RBAC to control which teams can create claims in which namespaces.

#### Self-Service Guardrails

Build guardrails into XRDs to prevent misconfiguration:

- Use enums to restrict choices (small/medium/large, not arbitrary values)
- Set min/max constraints on storage, replicas, retention periods
- Provide sensible defaults for optional parameters
- Use regex patterns for naming conventions
- Document expected values in field descriptions

#### Claim Lifecycle Management

Design claims for day-2 operations:

- Enable updates without replacement (use forProvider.applyMethod)
- Support scaling operations through parameter changes
- Include backup/restore configuration from day 1
- Plan for disaster recovery scenarios
- Document which parameters can be changed post-creation

#### Cost Visibility

Make cost implications visible to claim users:

- Add cost-related metadata to XRD descriptions
- Use labels for cost allocation (team, project, environment)
- Document size tiers with approximate costs
- Implement budget controls through validation webhooks
- Export cost tags to cloud provider billing

### Provider Configuration Strategies

#### Multi-Account Architecture

Use separate ProviderConfigs for different accounts/environments:

- Isolate prod from non-prod at the cloud account level
- Use IRSA (IAM Roles for Service Accounts) for AWS authentication
- Configure Workload Identity for GCP
- Implement Managed Identities for Azure

Reference the appropriate ProviderConfig in compositions or allow claims to specify it.

#### Credential Rotation

Implement secure credential management:

- Use external secret stores (Vault, AWS Secrets Manager)
- Configure ESO (External Secrets Operator) integration
- Rotate credentials on a schedule
- Use short-lived credentials when possible
- Avoid storing credentials in git

#### Provider Scoping

Install provider families strategically:

- Use scoped providers (provider-aws-s3) not monolithic (provider-aws)
- Reduces memory footprint and reconciliation load
- Install only required provider families
- Configure separate controller replicas for high-volume families
- Tune poll intervals per provider (--poll flag)

#### Rate Limiting

Configure provider controllers for production scale:

- Set --max-reconcile-rate based on API quotas
- Configure --poll interval to balance freshness vs load
- Use --enable-management-policies for granular control
- Monitor provider controller CPU/memory usage
- Scale controller replicas for high resource counts

### XRD Design

#### Keep XRDs Simple and Focused

- Each XRD should represent a single logical resource type
- Avoid combining unrelated infrastructure into one XRD
- Use composition to build complex platforms from simple XRDs

#### Version Your APIs

- Start with v1alpha1 and evolve to v1beta1, then v1
- Use multiple versions to support backwards compatibility
- Document breaking changes clearly

#### Define Clear Schemas

- Use OpenAPI validation (enums, patterns, min/max)
- Provide sensible defaults
- Mark required fields explicitly
- Add descriptions to all fields

#### Connection Secrets

- Only expose necessary connection details
- Use consistent key names across XRDs
- Document expected secret keys

### Composition Guidelines

#### Resource Naming

- Use deterministic names based on composite UID
- Avoid conflicts with CombineFromComposite patches
- Consider external name requirements

#### Patch Strategies

- Use PatchSets for common patches
- Apply FromCompositeFieldPath for user inputs
- Use ToCompositeFieldPath for status updates
- Leverage transforms (map, string formatting, math)

#### Resource Dependencies

- Use selectors (matchControllerRef, matchLabels) for references
- Crossplane handles dependency ordering automatically
- Avoid circular dependencies

#### Environment-Specific Logic

- Use map transforms to vary resources by environment
- Example: small instances for dev, large for prod
- Map transforms can only vary field **values** on resources that are always created — conditional resource **inclusion** (create a cache only if `enableCache=true`) requires `mode: Pipeline` with a templating function (`function-go-templating` or `function-kcl`); patch-and-transform intentionally has no loops/conditionals

#### Connection Secret Propagation

- Write secrets to crossplane-system namespace first
- Copy to claim namespace using patches
- Merge secrets from multiple resources

### Claim Organization

#### Namespace Strategy

- One namespace per team or environment
- Use RBAC to control claim creation
- Claims are namespace-scoped, XRs are cluster-scoped

#### Naming Conventions

- Use descriptive claim names (app-name-db, not db-1)
- Include environment in name if not using namespace separation
- Follow organization naming standards

#### Labels and Annotations

- Add ownership labels (team, cost-center)
- Use annotations for metadata (jira-ticket, owner-email)
- Labels can be used in composition patches

### ProviderConfig Best Practices

#### Multiple Provider Configs

- Use different ProviderConfigs for different accounts/projects
- Name them descriptively (prod-aws, dev-aws)
- Reference explicitly in compositions or claims

#### Credential Management

- Use IRSA (IAM Roles for Service Accounts) when possible
- Store credentials in secrets with minimal permissions
- Rotate credentials regularly

#### Resource Limits

- Configure provider controller resource limits
- Set appropriate poll intervals (--poll flag)
- Limit max reconcile rate for large deployments

### Production Readiness

#### Deletion Policies

- **`deletionPolicy` defaults to `Delete`** — deleting the Kubernetes object destroys the real cloud resource. Set `deletionPolicy: Orphan` **from day one** on ANY resource whose accidental deletion causes data loss or an outage (databases, buckets, volumes), not only production databases.
- Use `deletionPolicy: Delete` only for genuinely disposable dev/test resources
- In newer Crossplane, deletion is increasingly expressed via `managementPolicies` (omitting `Delete` prevents external deletion), but `deletionPolicy: Delete` remains the default disposition
- Document deletion behavior for platform users

#### Resource Tagging

- Tag all resources with ManagedBy: crossplane
- Include environment, team, and cost allocation tags
- Propagate tags from composite to managed resources

#### Monitoring and Observability

- Monitor Crossplane controller metrics
- Set up alerts for failed reconciliations
- Export provider metrics to your monitoring system
- Check resource sync status regularly

#### Testing

- Test compositions in dev before promoting to prod
- Validate XRDs with kube-linter or similar tools
- Use dry-run mode for risky changes

#### Documentation

- Document XRD schemas with examples
- Provide claim templates for platform users
- Maintain composition change logs

#### Security

- Use least-privilege IAM policies
- Enable encryption at rest and in transit
- Use private endpoints where possible
- Implement network security groups/firewalls

## Common Operations

### Debugging

```bash
# Check Crossplane status
kubectl get crossplane

# View provider status
kubectl get providers

# Check managed resources
kubectl get managed

# View composite resources
kubectl get composite

# Describe a claim to see events
kubectl describe postgresqlinstance my-app-db -n production

# View composition functions
kubectl get functions

# Check provider logs
kubectl logs -n crossplane-system -l pkg.crossplane.io/provider=provider-aws-s3

# Get all resources created by a composition
kubectl get managed -l crossplane.io/composite=<composite-name>
```

### Troubleshooting

```bash
# Check if provider is healthy
kubectl get providers
kubectl describe provider provider-aws-s3

# Verify ProviderConfig
kubectl get providerconfigs
kubectl describe providerconfig default

# Check for reconciliation errors
kubectl describe <resource-type> <resource-name>

# View conditions
kubectl get <resource> <name> -o jsonpath='{.status.conditions}'

# Test claim creation
kubectl apply -f claim.yaml --dry-run=server

# Validate XRD
kubectl apply -f xrd.yaml --dry-run=server
```

### Updating Resources

```bash
# Update a composition: with the default Automatic update policy this IMMEDIATELY
# reconciles ALL existing composites (not just new ones). Set
# compositionUpdatePolicy: Manual on XRs, or defaultCompositionUpdatePolicy: Manual
# on the XRD, to gate the rollout and promote tested revisions deliberately.
kubectl apply -f composition.yaml

# Pause / unpause reconciliation. WARNING: a paused resource
# (crossplane.io/paused: "true") cannot be deleted with kubectl delete until the
# annotation is removed — always unpause before deleting. Only the exact string
# "true" pauses; "false" does NOT force a reconcile, it just clears the pause.
kubectl annotate managed my-rds-instance crossplane.io/paused-   # remove pause

# Update XRD (be careful with breaking changes)
kubectl apply -f xrd.yaml

# Upgrade provider
kubectl apply -f provider.yaml  # with new version
```

## Advanced Patterns

### Multi-Region Deployments

```yaml
# Create multiple compositions, one per region
apiVersion: apiextensions.crossplane.io/v1
kind: Composition
metadata:
  name: postgres.us-west-2.aws.database.example.com
  labels:
    provider: aws
    region: us-west-2
spec:
  compositeTypeRef:
    apiVersion: database.example.com/v1alpha1
    kind: XPostgreSQLInstance
  # ... resources configured for us-west-2
---
# Claim with region selector
apiVersion: database.example.com/v1alpha1
kind: PostgreSQLInstance
metadata:
  name: my-db
spec:
  compositionSelector:
    matchLabels:
      region: us-west-2
```

### Blue-Green Deployments

```yaml
# Use labels to manage active/inactive compositions
apiVersion: apiextensions.crossplane.io/v1
kind: Composition
metadata:
  name: app-v2
  labels:
    version: v2
    active: "true"
spec:
  # ... new composition
---
# Claim selects active version
spec:
  compositionSelector:
    matchLabels:
      active: "true"
```

### Conditional Resource Creation

Use Composition Functions to conditionally include resources based on input parameters:

```yaml
# compositions/conditional-cache-composition.yaml
apiVersion: apiextensions.crossplane.io/v1
kind: Composition
metadata:
  name: appplatform-with-conditional-cache
spec:
  compositeTypeRef:
    apiVersion: platform.example.com/v1alpha1
    kind: XAppPlatform

  mode: Pipeline
  pipeline:
    # Step 1: Create base resources
    - step: create-resources
      functionRef:
        name: function-patch-and-transform
      input:
        apiVersion: pt.fn.crossplane.io/v1beta1
        kind: Resources
        resources:
          - name: storage-bucket
            base:
              apiVersion: s3.aws.upbound.io/v1beta1
              kind: Bucket
              spec:
                forProvider:
                  region: us-west-2

    # Step 2: Conditionally add cache using function-kcl or function-go-templating
    - step: add-cache-if-enabled
      functionRef:
        name: function-go-templating
      input:
        apiVersion: gotemplating.fn.crossplane.io/v1beta1
        kind: GoTemplate
        source: Inline
        inline:
          template: |
            {{ if .observed.composite.resource.spec.parameters.enableCache }}
            apiVersion: elasticache.aws.upbound.io/v1beta1
            kind: ReplicationGroup
            metadata:
              name: {{ .observed.composite.resource.metadata.name }}-cache
              annotations:
                gotemplating.fn.crossplane.io/composition-resource-name: cache
            spec:
              forProvider:
                description: Redis cache
                engine: redis
                engineVersion: "7.0"
                nodeType: cache.t3.micro
                numCacheClusters: 1
            {{ end }}

    # Step 3: Mark resources ready
    - step: auto-ready
      functionRef:
        name: function-auto-ready
```

Alternative approach using separate compositions:

```yaml
# Create two compositions: one with cache, one without
# composition-with-cache.yaml
metadata:
  labels:
    cache: enabled
# ... includes cache resources

# composition-without-cache.yaml
metadata:
  labels:
    cache: disabled
# ... excludes cache resources

# Claim selects the appropriate composition
spec:
  compositionSelector:
    matchLabels:
      cache: enabled  # or disabled
```

### Cost Optimization

```yaml
# Use environment-based sizing
patches:
  - type: FromCompositeFieldPath
    fromFieldPath: spec.parameters.environment
    toFieldPath: spec.forProvider.instanceClass
    transforms:
      - type: map
        map:
          dev: db.t3.micro # $0.017/hour
          staging: db.t3.small # $0.034/hour
          prod: db.m5.large # $0.192/hour
```

## Migration Strategies

### Importing Existing Resources

```yaml
# Import existing infrastructure
apiVersion: s3.aws.upbound.io/v1beta1
kind: Bucket
metadata:
  name: existing-bucket
  annotations:
    crossplane.io/external-name: my-existing-bucket-name
spec:
  forProvider:
    region: us-west-2
  providerConfigRef:
    name: default
  # Crossplane will discover and manage this existing bucket
```

### Migrating from Terraform

1. Export Terraform state for resources
2. Create equivalent managed resources with matching external names
3. Import into Crossplane using external-name annotation
4. Gradually build compositions around managed resources
5. Migrate teams to claims

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance distilled from production Crossplane and the official docs. Each item states the mechanism, not just the rule.

### Currency (Crossplane v2)

**v2 removed `mode: Resources` — all Compositions must use `mode: Pipeline`.** In v1 the patch-and-transform engine was embedded in the core binary; v2 extracted it into `function-patch-and-transform`, decoupling it from the core release cycle. A Composition that omits `mode:` (implicitly `Resources`) or sets `mode: Resources` is rejected on v2. Convert with `crossplane beta convert pipeline-composition old.yaml -o new.yaml` (v1.20 CLI) before upgrading.

**`ControllerConfig` removed — use `DeploymentRuntimeConfig` + `runtimeConfigRef`.** A provider still referencing `controllerConfigRef` has its runtime config (poll interval, security context, reconcile rate) **silently ignored** on v2. `DeploymentRuntimeConfig` exposes the full pod template (args, env, limits, security context, ServiceAccount) and is strictly more capable. See the AWS provider example above.

**v2 XRDs are namespaced-by-default and drop claims (`apiextensions.crossplane.io/v2`).** The new `scope` field defaults to `Namespaced`; namespaced/cluster XRs don't support claims (`claimNames` is gone), and `connectionSecretKeys` no longer applies. v1 XRDs default to `LegacyCluster` and keep working with claims for backward compatibility. Prefer v2 + `scope: Namespaced` for new XRDs — it aligns with standard Kubernetes RBAC/multi-tenancy.

**v2 universal composition: an XR can compose ANY Kubernetes resource.** Not just managed resources — a single XR can bundle infrastructure (RDS) with application-layer resources (Deployments, ServiceAccounts, NetworkPolicies, operator CRs). Requires `mode: Pipeline` and v2 namespaced XRDs, where the namespace boundary co-locates composed resources.

```yaml
resources:
  - name: rds-instance
    base:
      apiVersion: rds.aws.m.upbound.io/v1beta1
      kind: Instance
  - name: app-service-account
    base:
      apiVersion: v1
      kind: ServiceAccount   # plain Kubernetes resource, composed by the XR
```

**Registry defaults changed twice.** v1.20 moved the crossplane-contrib default registry from `xpkg.upbound.io` to the neutral `xpkg.crossplane.io`. Crossplane v2 then **dropped the `--registry` default entirely** — bare image names fail; always use a fully qualified URL. Upbound's own providers (`provider-aws`, etc.) remain on `xpkg.upbound.io`.

### Migration

**Run `crossplane beta upgrade check` (v1.20 CLI) before any v1.x → v2 migration.** This read-only scan names every deprecated/removed feature that would break the upgrade (Resources-mode compositions, `ControllerConfig`s, bare package names) and the exact `crossplane beta convert` sub-command to fix each. Supports `-o json` for CI and exits non-zero on blockers. The upgrade path is stepwise — reach v1.20 first, then advance one minor at a time. (`beta upgrade`/`beta convert` are v1.20 pre-upgrade tooling, absent from the v2 CLI.)

```bash
crossplane --version            # confirm 1.20.x
crossplane beta upgrade check    # read-only scan for v2 blockers
crossplane beta convert pipeline-composition composition.yaml -o composition-v2.yaml
```

### Anti-Patterns

**Never rebuild the desired-resource map from scratch in a pipeline function.** Each function receives the accumulated desired state from all prior steps, and Crossplane applies the desired state returned by the **last** function. The contract is **get → update → set**: get existing desired resources, add/modify your own, set the complete map back. Initializing an empty map and setting only your own resources drops everything prior steps added — Crossplane then garbage-collects (deletes) the corresponding cloud resources.

```go
// CORRECT: get -> update -> set preserves prior steps' resources
desired, err := request.GetDesiredComposedResources(req)
desired["my-bucket"] = &resource.DesiredComposed{Resource: cd}
response.SetDesiredComposedResources(rsp, desired)
// WRONG: desired := map[...]{} then set -> deletes everything else
```

### Design Patterns

**Import existing cloud resources with `managementPolicies: [Observe]` + `external-name` first.** Full management treats `spec.forProvider` as authoritative and drift-corrects on the next reconcile — mutable fields are changed on the live resource with no confirmation (e.g. silently resizing a `db.m5.large` to `db.t3.micro`), immutable fields cause a reconcile error loop. Safe sequence: (1) set `crossplane.io/external-name: <cloud-id>` and `managementPolicies: [Observe]`; (2) apply and let `status.atProvider` populate; (3) copy discovered values into `spec.forProvider`; (4) switch to `managementPolicies: ["*"]`. Management policies (`Create`/`Delete`/`Update`/`Observe`/`LateInitialize`/`*`) are finer-grained than the blunt `crossplane.io/paused` annotation; an empty array `[]` behaves like paused.

**Hard-lock the composition with `enforcedCompositionRef` in multi-tenant platforms.** Consumers can set `compositionRef`/`compositionSelector` to bypass the intended composition. `enforcedCompositionRef` in the XRD binds **all** XRs of that type to one named Composition regardless of consumer choice — the governance control for platform teams. This differs from `defaultCompositionRef`, which is merely an overridable fallback.

**Use `function-extra-resources` for cross-resource lookups, not nested XRs.** To read another cluster resource (an `EnvironmentConfig`, a VPC config, another XR's status), `function-extra-resources` fetches matches by name/label selector into the pipeline context under `apiextensions.crossplane.io/extra-resources`; downstream functions (`function-go-templating`, `function-kcl`) read that key. Nesting an XR to access shared config creates ownership coupling and deletion-ordering complexity.

**`patch-and-transform` has no loops/conditionals — chain `function-kcl` or `function-go-templating`.** P&T is intentionally limited to straightforward field mapping. For conditional resource creation, iteration over lists, or complex string logic, add a templating-function step in the same pipeline (run P&T for field mapping, then the templating function for dynamic logic).

### Gotchas

**`compositionUpdatePolicy` defaults to `Automatic` — editing a Composition immediately reconciles ALL live XRs.** Applying a changed Composition pushes the new template to every referencing XR at once (uncontrolled blast radius). Even label/annotation edits create a new `CompositionRevision`, so cosmetic changes can trigger a rolling reconcile. Production-safe: set `defaultCompositionUpdatePolicy: Manual` on the XRD (or `compositionUpdatePolicy: Manual` per XR), pin tested revisions via `compositionRevisionRef.name`, and promote deliberately (canary/per-env). Note `compositeTypeRef.apiVersion` is immutable on a Composition, so version migrations require new Composition objects.

**Delete claims/XRs before the Provider — never the reverse.** Deleting a Provider while its managed resources still exist leaves them **permanently stuck**: `finalizer.managedresource.crossplane.io` blocks Kubernetes garbage collection, but the controller that would process deletion is gone. Recovery needs manual finalizer surgery (`kubectl patch <mr> -p '{"metadata":{"finalizers":[]}}' --type=merge`) plus manual cloud-side cleanup. Safe teardown: `kubectl delete <claim/xr>`, `kubectl wait --for=delete managed --all`, then delete the Provider.

**A paused managed resource cannot be deleted.** `crossplane.io/paused: "true"` halts all provider reconciliation including deletion — `kubectl delete` hangs in `Terminating` forever. Only the exact string `"true"` pauses; `"false"` does not force a reconcile, it just clears the pause. Always `kubectl annotate ... crossplane.io/paused-` before deleting.

**`FromCompositeFieldPath` patches default to `Optional` — a typo'd source path is silently skipped.** Crossplane ignores a patch whose `fromFieldPath` doesn't exist, so a mistyped field or a not-yet-populated status field silently provisions the resource with the composition's base value, no error or event. For any load-bearing patch, set `policy.fromFieldPath: Required` to surface the misconfiguration.

```yaml
patches:
  - type: FromCompositeFieldPath
    fromFieldPath: spec.parameters.size
    toFieldPath: spec.forProvider.instanceClass
    policy:
      fromFieldPath: Required   # error if absent instead of silent skip
```

**`function-auto-ready` can mark an XR Ready before conditional resources exist.** It marks the composite Ready when all resources **currently in the desired state** report `Ready=True`. If a later pipeline step conditionally adds resources (e.g. a cache created only after another resource's status appears), auto-ready sees only the current output and can mark the XR Ready in the interim, letting dependents proceed too early. For compositions with conditional resources, add explicit `readinessChecks` on the relevant composed resources rather than relying solely on auto-ready.

### Idioms

**Composition functions must produce deterministic resource names.** A function runs on every reconcile, and Crossplane matches composed resources by their name key. A name derived from a random/time value changes between reconciles — Crossplane sees the old name as deleted and the new as created, causing delete+create churn of real infrastructure. Derive names from stable XR fields (`GetName()`/UID or a stable hash of stable inputs).

**Use `Fatal` vs `Warning` vs `Normal` correctly — wrong severity blocks or hides errors.** `Fatal` stops the pipeline and surfaces an error on the XR (reserve for unrecoverable input/programming errors); `Warning` emits a Kubernetes event but reconciliation continues (use for recoverable external state, e.g. a referenced resource not yet ready); `Normal` is informational. Using `Fatal` for a transient condition halts ALL reconciliation.

**Prefer the `match` transform with `fallbackTo` over `map` for enum mapping.** `map` has no implicit fallback — `Crossplane throws an error if the value isn't found`, so adding a new enum value to the XRD before updating the map breaks the composition. `match` supports `fallbackTo: Value` (with `fallbackValue`) or `fallbackTo: Input`, giving an explicit default. Use `match` for any mapping whose input set may grow.

```yaml
transforms:
  - type: match
    match:
      patterns:
        - { type: literal, literal: small, result: db.t3.micro }
        - { type: literal, literal: large, result: db.m5.large }
      fallbackTo: Value
      fallbackValue: db.t3.micro   # safe default instead of an error
```

**Test pipelines locally with `crossplane composition render`.** `crossplane composition render xr.yaml composition.yaml functions.yaml` renders composed resources with no cluster; `--observed-resources` mocks existing state to test idempotency. For active function development, annotate the Function with `render.crossplane.io/runtime: Development` and run the function locally (listening on `localhost:9443` with `--insecure`) to eliminate the build/push/install loop.

### Performance

**Use `ManagedResourceActivationPolicy` (MRAP) to selectively activate provider CRDs in v2.** Large family providers ship hundreds of CRDs that inflate API-server load even when few are used. v2 introduces ManagedResourceDefinitions for selective activation: only activated MRDs get CRDs installed and controllers started, selected via an MRAP (exact names or wildcards). This supersedes the v1 workaround of installing many individually-scoped providers. (MRAP/MRD are new and evolving — verify the exact `apiVersion` and the namespaced `.m.` MRD naming against your installed version.)

```yaml
apiVersion: pkg.crossplane.io/v1alpha1
kind: ManagedResourceActivationPolicy
metadata:
  name: activate-aws-essentials
spec:
  activate:
    - buckets.s3.aws.m.upbound.io
    - "*.rds.aws.m.upbound.io"
```

## References

- [Crossplane Documentation](https://docs.crossplane.io)
- [Upgrade to Crossplane v2](https://docs.crossplane.io/latest/guides/upgrade-to-crossplane-v2/)
- [Upbound Providers](https://marketplace.upbound.io)
- [Composition Functions](https://docs.crossplane.io/latest/concepts/composition-functions)
- [AWS Provider](https://marketplace.upbound.io/providers/upbound/provider-aws)
- [GCP Provider](https://marketplace.upbound.io/providers/upbound/provider-gcp)
- [Azure Provider](https://marketplace.upbound.io/providers/upbound/provider-azure)
