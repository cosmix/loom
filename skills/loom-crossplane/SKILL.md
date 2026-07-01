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

The XRD **is your platform API** — the OpenAPI schema is your only guardrail against consumer misconfiguration. Enforce with `enum`, `minimum`/`maximum`, `pattern`, `default`, and `required`. `spec.group` and `spec.names` are **immutable**: changing them requires deleting/recreating the XRD, which cascades to all its XRs.

**v1 (below) vs v2:** the v1 form uses `claimNames` (cluster-scoped XR + namespaced claim) and `connectionSecretKeys`. New XRDs should use `apiextensions.crossplane.io/v2` — **namespaced by default** (`scope: Namespaced`), **no claims** (`claimNames` gone), and `connectionSecretKeys` **does not apply** (aggregate via `function-patch-and-transform`'s `writeConnectionSecretToRef.patches`).

```yaml
# xrds/database-xrd.yaml  (v1 legacy-cluster form with claims)
apiVersion: apiextensions.crossplane.io/v1
kind: CompositeResourceDefinition
metadata:
  name: xpostgresqlinstances.database.example.com   # must be <plural>.<group>
spec:
  group: database.example.com
  names: { kind: XPostgreSQLInstance, plural: xpostgresqlinstances }
  claimNames: { kind: PostgreSQLInstance, plural: postgresqlinstances }  # v1 only
  connectionSecretKeys: [username, password, endpoint, port]             # v1 only
  versions:
    - name: v1alpha1
      served: true
      referenceable: true          # exactly one version must be referenceable
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
                    storageGB: { type: integer, default: 20, minimum: 20, maximum: 1000 }
                    size: { type: string, enum: [small, medium, large], default: small }
                    networkRef:      # nested object with its own required field
                      type: object
                      properties: { id: { type: string } }
                      required: [id]
                  required: [size, networkRef]
              required: [parameters]
            status:                  # ToCompositeFieldPath patches write here
              type: object
              properties:
                address: { type: string }
```

For the **v2** form, drop `claimNames`/`connectionSecretKeys` and add `scope: Namespaced` (XRs then live in a namespace and follow standard Kubernetes RBAC):

```yaml
apiVersion: apiextensions.crossplane.io/v2
kind: CompositeResourceDefinition
spec:
  scope: Namespaced        # default in v2; also LegacyCluster (v1 behavior) or Cluster
  group: platform.example.com
  names: { kind: XAppPlatform, plural: xappplatforms }
  # ... versions/schema as above
```

## Compositions

A Composition implements an XRD's API by templating one or more resources. **On v2, `mode: Pipeline` (with `function-patch-and-transform`) is the ONLY supported form** — native `mode: Resources` (`spec.resources`/`spec.patchSets`) was deprecated in v1.17 and removed in v2. See the full Pipeline example under *Composition Functions*; migrate legacy compositions with `crossplane beta convert pipeline-composition old.yaml -o new.yaml`.

### Patch & Transform vocabulary

These patch/transform types are the workhorses of both legacy `mode: Resources` **and** `function-patch-and-transform` input — the syntax is identical, so knowledge transfers verbatim.

| Patch `type`            | Direction         | Use                                                     |
| ----------------------- | ----------------- | ------------------------------------------------------ |
| `FromCompositeFieldPath`| XR → composed MR  | Push a user parameter onto a managed resource field    |
| `ToCompositeFieldPath`  | composed MR → XR  | Surface `status.atProvider.*` back to the XR status    |
| `CombineFromComposite`  | many XR → one MR  | Build a value (e.g. name) from multiple fields via `fmt`|
| `PatchSet` (+`patchSets`)| —                | Reuse a named patch group across resources (e.g. common tags)|

| Transform `type` | Use / gotcha                                                                 |
| ---------------- | ---------------------------------------------------------------------------- |
| `map`            | Enum → value (`small`→`db.t3.micro`). **Errors if key absent** — prefer `match` with `fallbackTo` (see *Expert Practices → Idioms*) |
| `string`         | `fmt: "%s-connection"` for derived names — must stay deterministic across reconciles |
| `math`           | Scale a numeric field                                                         |
| `convert`        | Type coercion (string↔int↔bool)                                              |

```yaml
# Inside mode: Pipeline -> function-patch-and-transform input (see Composition Functions)
resources:
  - name: rds-instance
    base:
      apiVersion: rds.aws.upbound.io/v1beta1
      kind: Instance
      spec:
        forProvider:
          engine: postgres
          username: dbadmin
          # Selectors let Crossplane resolve refs & order deps automatically:
          vpcSecurityGroupIdSelector: { matchControllerRef: true }
          dbSubnetGroupNameSelector: { matchControllerRef: true }
        writeConnectionSecretToRef: { namespace: crossplane-system }
    patches:
      - type: PatchSet
        patchSetName: common-tags
      - type: FromCompositeFieldPath
        fromFieldPath: spec.parameters.size
        toFieldPath: spec.forProvider.instanceClass
        transforms:
          - type: map
            map: { small: db.t3.micro, medium: db.t3.medium, large: db.m5.large }
      - type: FromCompositeFieldPath          # derive a stable secret name from UID
        fromFieldPath: metadata.uid
        toFieldPath: spec.writeConnectionSecretToRef.name
        transforms: [{ type: string, string: { fmt: "%s-connection" } }]
      - type: ToCompositeFieldPath            # surface endpoint to XR status
        fromFieldPath: status.atProvider.endpoint
        toFieldPath: status.address
      - type: FromCompositeFieldPath          # LOAD-BEARING patch: fail loud, don't skip
        fromFieldPath: spec.parameters.networkRef.id
        toFieldPath: spec.forProvider.vpcId
        policy: { fromFieldPath: Required }
```

**Environment-driven variation** (dev/staging/prod) is just a `map` transform per field (e.g. `environment → multiAz`, `→ numCacheClusters`, `→ backupRetentionDays`). Conditional resource **inclusion** (create a cache only when `enableCache=true`) is NOT expressible in patch-and-transform — it needs a templating function (see *Conditional Resource Creation* and *Expert Practices*).

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

Community functions live on the neutral registry `xpkg.crossplane.io` (the default
for crossplane-contrib since v1.20). Upbound's own providers stay on `xpkg.upbound.io`.
Crossplane v2 has no default registry — always use a fully qualified URL.

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

Deep failure-mode guidance lives in *Expert Practices*; these are the high-value design defaults.

**Abstraction layering.** Foundation (provider MRs) → Resource XRDs (cloud-agnostic Database/ObjectStorage) → Platform XRDs (AppPlatform). Consume at the right level; keep each XRD to one logical resource type.

**Dependencies & ordering.** Prefer selectors (`matchControllerRef`, `matchLabels`) over explicit refs — Crossplane infers ordering and provisions in parallel when independent. Never add artificial ordering; avoid circular refs.

**XRD as API (guardrails).** The OpenAPI schema is your only misconfiguration guard: `enum` for choices, `minimum`/`maximum`, `pattern` for names, sensible `default`s, explicit `required`, descriptions on every field. Version `v1alpha1 → v1beta1 → v1`, keeping old versions served during migration. Expose only necessary `connectionSecretKeys` (v1) with consistent names.

**Composed-resource naming must be deterministic** (derive from XR `GetName()`/UID) — a random/time-derived name churns real infra every reconcile (see *Idioms*).

**Provider scoping & tuning.** Install scoped providers (`provider-aws-s3`) not the monolith — smaller memory/reconcile footprint. Tune per provider via `DeploymentRuntimeConfig` args: `--max-reconcile-rate` (respect cloud API quotas), `--poll` (freshness vs load). In v2 prefer `ManagedResourceActivationPolicy` (see *Performance*).

**Credentials.** Prefer workload identity over static keys — IRSA (AWS), Workload Identity (GCP), Managed Identity (Azure). For static secrets use ESO/Vault, least-privilege, never in git. Separate `ProviderConfig` per account/env (`prod-aws`, `dev-aws`) to isolate blast radius.

**Multi-tenancy.** One namespace per team/env + RBAC on claim creation. In multi-tenant platforms, pin the Composition with `enforcedCompositionRef` so consumers can't select a rogue one (see *Design Patterns*).

**Deletion safety (critical).** `deletionPolicy` defaults to **`Delete`** — deleting the k8s object destroys the real cloud resource. Set `deletionPolicy: Orphan` (or `managementPolicies` omitting `Delete`) **from day one** on ANYTHING stateful (DBs, buckets, volumes), not just prod. Delete claims/XRs **before** their Provider (see *Gotchas*).

**Ops hygiene.** Tag everything `ManagedBy: crossplane` + cost/team labels; monitor controller metrics and failed-reconcile events; test with `crossplane composition render` (no cluster) before promoting; encrypt at rest/in transit; least-privilege IAM.

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

> ⚠ Importing with full management (`managementPolicies: ["*"]`) treats `spec.forProvider`
> as authoritative and **drift-corrects on the next reconcile** — it can silently resize a
> live DB. Always import with `managementPolicies: [Observe]` first, copy discovered values
> into `spec.forProvider`, then promote to full management (see *Design Patterns*).

### Migrating from Terraform

**Model difference:** Terraform is imperative-ish `plan`/`apply` (drift is detected only when you next run); Crossplane **continuously reconciles** — a control loop drives real infra back to declared state on every sync, with no human `apply` in the loop. This is more self-healing but means an unintended `spec` change (or a bad Composition rollout) propagates immediately to production. Gate that blast radius with `compositionUpdatePolicy: Manual` + pinned revisions (see *Gotchas*).

1. Export Terraform state; note each resource's cloud ID.
2. Create equivalent MRs with `crossplane.io/external-name: <cloud-id>` and `managementPolicies: [Observe]`.
3. Let `status.atProvider` populate, reconcile `spec.forProvider` to match, then switch to `["*"]`.
4. Build Compositions around the imported MRs; migrate teams to claims/XRs last.

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

## Verification Checklist

Before shipping an XRD/Composition/Provider change:

- [ ] `crossplane composition render xr.yaml composition.yaml functions.yaml` renders the expected resources (add `--observed-resources` to check idempotency)
- [ ] Composition is `mode: Pipeline` (required on v2); no `mode: Resources`, no `ControllerConfig`, no bare (unqualified) package names
- [ ] Every load-bearing `FromCompositeFieldPath` sets `policy.fromFieldPath: Required` (defaults to silently-skipped `Optional`)
- [ ] Enum→value mappings use `match` + `fallbackTo` (not `map`, which errors on unknown keys)
- [ ] Composed-resource names are deterministic (derived from XR name/UID) — no random/time inputs
- [ ] `deletionPolicy: Orphan` (or `managementPolicies` without `Delete`) on every stateful resource
- [ ] `compositionUpdatePolicy: Manual` (or XRD `defaultCompositionUpdatePolicy: Manual`) + pinned revision for prod, so edits don't reconcile all XRs at once
- [ ] Pipeline functions use get→update→set on desired resources (never rebuild the map from scratch)
- [ ] XRD schema has enum/min-max/pattern/required guardrails; `group`/`names` are final (immutable)
- [ ] Provider installed and `Healthy`; `ProviderConfig` credentials resolve; `crossplane beta upgrade check` clean before any v1→v2 move

## References

- [Crossplane Documentation](https://docs.crossplane.io)
- [Upgrade to Crossplane v2](https://docs.crossplane.io/latest/guides/upgrade-to-crossplane-v2/)
- [Upbound Providers](https://marketplace.upbound.io)
- [Composition Functions](https://docs.crossplane.io/latest/concepts/composition-functions)
- [AWS Provider](https://marketplace.upbound.io/providers/upbound/provider-aws)
- [GCP Provider](https://marketplace.upbound.io/providers/upbound/provider-gcp)
- [Azure Provider](https://marketplace.upbound.io/providers/upbound/provider-azure)
