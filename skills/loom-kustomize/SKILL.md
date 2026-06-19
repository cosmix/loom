---
name: loom-kustomize
description: Kubernetes-native configuration management with Kustomize. Use for environment-specific configs, resource patching (strategic merge, JSON6902), ConfigMap/Secret generation, overlays/bases, multi-environment deployments, and GitOps workflows.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - kustomize
  - kustomization
  - overlay
  - base
  - patch
  - strategic merge
  - json patch
  - json6902
  - configmap generator
  - secret generator
  - namespace
  - namePrefix
  - nameSuffix
  - commonLabels
  - commonAnnotations
  - component
  - transformer
  - replacement
  - multi-environment
  - dev/staging/prod configs
  - k8s manifest management
---

# Kustomize Skill

## Overview

Kustomize is a Kubernetes-native configuration management tool that uses declarative customization to manage environment-specific configurations without templates. It follows the principles of declarative application management and integrates directly with kubectl.

**Primary use cases:** Multi-environment deployments, resource patching, GitOps workflows, ConfigMap/Secret generation, cross-cutting transformations.

### What This Skill Covers

- **Multi-Environment Overlays**: Dev/staging/prod pattern with base + overlays
- **Patch Strategies**: Strategic merge vs JSON 6902 patches, when to use each
- **Generators**: ConfigMap and Secret generation with automatic content hashing
- **Transformers**: Cross-cutting changes (labels, annotations, namespace, namePrefix/Suffix, images, replicas)
- **Components**: Reusable optional feature bundles (monitoring, ingress, debug-tools)
- **Replacements**: Dynamic field substitution for propagating values between resources
- **GitOps Integration**: ArgoCD, Flux, CI/CD pipeline patterns
- **Security**: Secret management, image pinning, validation, RBAC patterns

### Core Concepts

- **Base**: A directory containing a `kustomization.yaml` and a set of resources (typically common/shared configurations)
- **Overlay**: A directory with a `kustomization.yaml` that refers to a base and applies customizations (environment-specific configs like dev/staging/prod)
- **Patch**: A partial resource definition that modifies existing resources (strategic merge or JSON patch)
- **Component**: Reusable customization bundles that can be included in multiple kustomizations (e.g., monitoring, ingress)
- **Generator**: Creates ConfigMaps and Secrets from files, literals, or env files with automatic content hashing
- **Transformer**: Modifies resources across the board (labels, annotations, namespaces, namePrefix, nameSuffix, replicas, images)
- **Replacement**: Dynamic field substitution that propagates values between resources (e.g., ConfigMap names with hash suffixes)

### Key Principles

1. **Bases are reusable**: Define common configuration once, customize per environment
2. **Overlays are composable**: Stack multiple customizations for different environments
3. **Resources are not modified**: Original base files remain unchanged
4. **No templating**: Uses declarative merging instead of variable substitution
5. **kubectl integration**: `kubectl apply -k <directory>` natively supports Kustomize
6. **Content hashing**: ConfigMaps and Secrets get automatic name suffixes based on content for immutable deployments

## Directory Structure

### Recommended Layout

```text
k8s/
├── base/
│   ├── kustomization.yaml
│   ├── deployment.yaml
│   ├── service.yaml
│   └── configmap.yaml
├── overlays/
│   ├── dev/
│   │   ├── kustomization.yaml
│   │   ├── patch-replicas.yaml
│   │   └── config-values.env
│   ├── staging/
│   │   ├── kustomization.yaml
│   │   ├── patch-replicas.yaml
│   │   └── config-values.env
│   └── prod/
│       ├── kustomization.yaml
│       ├── patch-replicas.yaml
│       ├── patch-resources.yaml
│       └── config-values.env
└── components/
    ├── monitoring/
    │   ├── kustomization.yaml
    │   └── servicemonitor.yaml
    └── ingress/
        ├── kustomization.yaml
        └── ingress.yaml
```

### Multi-Service Structure

```text
k8s/
├── base/
│   ├── kustomization.yaml (references all services)
│   ├── namespace.yaml
│   └── services/
│       ├── api/
│       │   ├── kustomization.yaml
│       │   ├── deployment.yaml
│       │   └── service.yaml
│       └── worker/
│           ├── kustomization.yaml
│           ├── deployment.yaml
│           └── service.yaml
└── overlays/
    ├── dev/
    │   └── kustomization.yaml
    ├── staging/
    │   └── kustomization.yaml
    └── prod/
        └── kustomization.yaml
```

## Quick Reference

### Common Operations

| Task             | Command                                                                    |
| ---------------- | -------------------------------------------------------------------------- |
| Build manifests  | `kustomize build k8s/overlays/prod`                                        |
| Preview changes  | `kubectl diff -k k8s/overlays/prod`                                        |
| Apply to cluster | `kubectl apply -k k8s/overlays/prod`                                       |
| Validate syntax  | `kustomize build k8s/overlays/prod \| kubectl apply --dry-run=client -f -` |
| Update image tag | `kustomize edit set image myapp=registry/myapp:v1.2.3`                     |
| Add resource     | `kustomize edit add resource deployment.yaml`                              |
| Add ConfigMap    | `kustomize edit add configmap app-config --from-literal=KEY=value`         |

### Multi-Environment Pattern

```text
k8s/
├── base/              # Shared configuration
├── overlays/
│   ├── dev/          # Development-specific (low resources, debug enabled)
│   ├── staging/      # Staging-specific (moderate resources, monitoring)
│   └── prod/         # Production-specific (high resources, HA, security)
└── components/       # Optional features (monitoring, ingress, debug-tools)
```

### Generator Quick Start

| Generator Type     | Use Case                | Example                                                                                   |
| ------------------ | ----------------------- | ----------------------------------------------------------------------------------------- |
| ConfigMap literals | Simple key-value config | `configMapGenerator: - name: app-config literals: - LOG_LEVEL=info`                       |
| ConfigMap files    | Config files            | `configMapGenerator: - name: app-config files: - application.properties`                  |
| ConfigMap env file | Environment variables   | `configMapGenerator: - name: app-config envs: - config.env`                               |
| Secret literals    | Simple secrets          | `secretGenerator: - name: app-secrets literals: - username=admin`                         |
| Secret files       | Certificate/key files   | `secretGenerator: - name: tls-secrets files: - tls.crt - tls.key type: kubernetes.io/tls` |

### Transformer Quick Start

| Transformer       | Use Case                         | Example                                    |
| ----------------- | -------------------------------- | ------------------------------------------ |
| namespace         | Set namespace for all resources  | `namespace: production`                    |
| namePrefix        | Add prefix to resource names     | `namePrefix: myapp-`                       |
| nameSuffix        | Add suffix to resource names     | `nameSuffix: -v2`                          |
| commonLabels      | Add labels to all resources      | `commonLabels: app: myapp team: platform`  |
| commonAnnotations | Add annotations to all resources | `commonAnnotations: managed-by: kustomize` |
| images            | Update container images          | `images: - name: myapp newTag: v1.2.3`     |
| replicas          | Set replica count                | `replicas: - name: myapp count: 5`         |

## Workflow

### 1. Create Base Configuration

Start with common resources that apply to all environments:

```bash
mkdir -p k8s/base
cd k8s/base
# Create resource files (deployment.yaml, service.yaml, etc.)
# Create kustomization.yaml to reference them
```

### 2. Build and Verify Base

```bash
kustomize build k8s/base
# or
kubectl kustomize k8s/base
```

### 3. Create Environment Overlays

```bash
mkdir -p k8s/overlays/dev
cd k8s/overlays/dev
# Create kustomization.yaml that references base
# Add patches and customizations
```

### 4. Apply to Cluster

```bash
# Preview changes
kubectl diff -k k8s/overlays/dev

# Apply
kubectl apply -k k8s/overlays/dev

# Delete
kubectl delete -k k8s/overlays/dev
```

### 5. Iterate and Refactor

- Extract common patterns to components
- Use generators for ConfigMaps and Secrets
- Apply transformers for cross-cutting concerns

## Patch Strategies

### Strategic Merge Patch (Default)

Strategic merge is the default patch strategy. It uses Kubernetes-aware merging logic.

#### Strategic Merge Characteristics

- Merges maps/objects by key
- Replaces arrays by default (unless special directives)
- Uses `$patch: delete` and `$patch: replace` directives
- More intuitive for Kubernetes resources

> **`$patch: replace` for entire-list replacement has been inconsistent across versions** (regressed in 3.8.x — issue #2980, where the directive was ignored and lists merged instead). For reliable whole-list replacement use a JSON 6902 patch with `op: replace` instead.

#### Strategic Merge Use Cases

- Simple field updates (replicas, image, env vars)
- Adding or replacing containers
- Updating resource limits
- Most common use case

### JSON Patch (RFC 6902)

JSON Patch provides precise array manipulation and field operations.

#### JSON Patch Characteristics

- Operations: add, remove, replace, move, copy, test
- Uses JSON Pointer paths (e.g., `/spec/template/spec/containers/0/image`)
- Precise array element targeting
- More verbose but more precise

#### JSON Patch Use Cases

- Precise array element manipulation
- Conditional patches (test operation)
- Complex nested updates
- When strategic merge is too coarse

## Component Patterns

Components are reusable customization bundles that can be selectively included in overlays. They're ideal for optional features that only some environments need.

### When to Use Components

| Pattern                 | Use Components For                | Use Patches For              |
| ----------------------- | --------------------------------- | ---------------------------- |
| Optional features       | Monitoring, ingress, debug tools  | Required modifications       |
| Cross-environment reuse | Feature available in staging+prod | Environment-specific changes |
| Clean composition       | Independent feature sets          | Tweaking existing resources  |
| Conditional inclusion   | Enable per environment            | Always apply in overlay      |

### Component Structure

```yaml
# k8s/components/monitoring/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1alpha1
kind: Component

resources:
  - servicemonitor.yaml
  - prometheusrule.yaml

patches:
  - path: patch-metrics.yaml
    target:
      kind: Deployment

labels:
  - pairs:
      prometheus.io/scrape: "true"
```

> **Always reference a Component under the `components:` field, never `resources:`.** A Component placed under `resources:` is silently misapplied as a plain manifest instead of composing as a feature bundle (kustomize even enforces that Components are not added to `resources:` and Kustomizations not added to `components:`).

### Including Components

```yaml
# k8s/overlays/prod/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  - ../../base

components:
  - ../../components/monitoring
  - ../../components/ingress
  - ../../components/security-hardening
```

### Common Component Patterns

**Monitoring Component**: ServiceMonitor, PrometheusRules, metrics port patch
**Ingress Component**: Ingress resource, TLS config, service annotations
**Debug Tools Component**: Debug env vars, profiling ports, verbose logging
**Security Hardening Component**: SecurityContext, NetworkPolicy, PodSecurityPolicy
**Backup Component**: CronJob for backups, PersistentVolume, ServiceAccount with backup permissions

### Component vs Overlay Decision Tree

```text
Need to include optional features? → Component
Need environment-specific values? → Overlay
Need both? → Component for features + Overlay patches for values
```

## Best Practices

### Directory Organization

1. **Keep bases generic**: Avoid environment-specific values in base
2. **One concern per patch**: Create separate patch files for different modifications
3. **Use descriptive names**: `patch-replicas.yaml`, `patch-monitoring.yaml`, not `patch1.yaml`
4. **Group related resources**: Keep services, deployments, and configs together
5. **Use components for features**: Extract optional features (monitoring, ingress) as components

### Patch Hygiene

1. **Minimize patch size**: Only include fields being changed
2. **Document complex patches**: Add comments explaining why patch is needed
3. **Prefer strategic merge**: Use JSON patch only when necessary
4. **Validate patches**: Run `kustomize build` to verify output
5. **Test combinations**: Ensure patches compose correctly

### Resource Management

1. **Use generators for dynamic data**: ConfigMaps and Secrets should use generators
2. **Enable name suffixes**: Add content hash to ConfigMap/Secret names for immutability
3. **Reference by resource**: Use `nameReference` for automatic name updates
4. **Common labels**: Apply consistent labels with the `labels` transformer (`includeSelectors: false`), not the deprecated `commonLabels`
5. **Namespace management**: Set namespace in kustomization, not individual resources. **Exception:** the namespace transformer runs in `DefaultSubjectsOnly` mode and only namespaces `RoleBinding`/`ClusterRoleBinding` subjects whose `name` is `default`. ServiceAccount subjects with any other name get no namespace — silently breaking RBAC — unless you configure a custom `NamespaceTransformer` with `setRoleBindingSubjects: allServiceAccounts`. Verify RBAC subjects explicitly after setting a namespace.

### Version Control

1. **Commit generated manifests**: Consider committing `kustomize build` output for GitOps
2. **Document dependencies**: Note any external resources or ordering requirements
3. **Pin versions**: Reference bases by version/tag when using remote bases
4. **Review rendered output**: Always check the final manifests before applying

### Security

1. **Never commit secrets**: Use sealed-secrets, external-secrets, or secret generators with gitignored files
2. **Use RBAC**: Limit who can modify base vs overlays
3. **Validate resources**: Use kustomize plugins or OPA for policy enforcement
4. **Separate sensitive overlays**: Consider separate repos for prod configurations

## Examples

### Basic Base Kustomization

#### k8s/base/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: myapp

# Use the labels transformer, NOT the deprecated commonLabels (which also
# mutates the immutable spec.selector.matchLabels). includeSelectors defaults
# to false, which is safe on live resources; includeTemplates reaches pod
# templates. Define genuine selector labels directly in deployment.yaml so the
# selector contract is explicit rather than silently injected.
labels:
  - pairs:
      app: myapp
      managed-by: kustomize
    includeSelectors: false
    includeTemplates: true

commonAnnotations:
  contact: team@example.com

resources:
  - deployment.yaml
  - service.yaml
  - serviceaccount.yaml

configMapGenerator:
  - name: app-config
    literals:
      - LOG_LEVEL=info
      - MAX_CONNECTIONS=100

images:
  - name: myapp
    newName: registry.example.com/myapp
    newTag: latest
```

#### k8s/base/deployment.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  replicas: 1
  selector:
    matchLabels:
      app: myapp
  template:
    metadata:
      labels:
        app: myapp
    spec:
      serviceAccountName: myapp
      containers:
        - name: app
          image: myapp
          ports:
            - containerPort: 8080
              name: http
          envFrom:
            - configMapRef:
                name: app-config
          resources:
            requests:
              memory: "128Mi"
              cpu: "100m"
            limits:
              memory: "256Mi"
              cpu: "200m"
          livenessProbe:
            httpGet:
              path: /health
              port: http
            initialDelaySeconds: 30
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /ready
              port: http
            initialDelaySeconds: 5
            periodSeconds: 5
```

#### k8s/base/service.yaml

```yaml
apiVersion: v1
kind: Service
metadata:
  name: myapp
spec:
  type: ClusterIP
  ports:
    - port: 80
      targetPort: http
      protocol: TCP
      name: http
  selector:
    app: myapp
```

#### k8s/base/serviceaccount.yaml

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: myapp
```

### Development Overlay

#### k8s/overlays/dev/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: myapp-dev

namePrefix: dev-
nameSuffix: -v1

# Environment labels via the labels transformer with includeSelectors: false —
# commonLabels would inject these into immutable selectors and break rollouts.
labels:
  - pairs:
      environment: dev
      version: v1
    includeSelectors: false
    includeTemplates: true

resources:
  - ../../base

patches:
  - path: patch-replicas.yaml
    target:
      kind: Deployment
      name: myapp

configMapGenerator:
  - name: app-config
    behavior: merge
    literals:
      - LOG_LEVEL=debug
      - ENABLE_DEBUG_ROUTES=true
    envs:
      - config-values.env

images:
  - name: myapp
    newTag: dev-latest

replicas:
  - name: myapp
    count: 1
```

#### k8s/overlays/dev/patch-replicas.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  replicas: 1
  template:
    spec:
      containers:
        - name: app
          resources:
            requests:
              memory: "64Mi"
              cpu: "50m"
            limits:
              memory: "128Mi"
              cpu: "100m"
```

#### k8s/overlays/dev/config-values.env

```text
DATABASE_URL=postgres://localhost:5432/myapp_dev
REDIS_URL=redis://localhost:6379
ENABLE_PROFILING=true
```

### Staging Overlay

#### k8s/overlays/staging/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: myapp-staging

commonLabels:
  environment: staging

resources:
  - ../../base

patches:
  - path: patch-replicas.yaml
  - path: patch-tolerations.yaml

configMapGenerator:
  - name: app-config
    behavior: merge
    envs:
      - config-values.env

secretGenerator:
  - name: app-secrets
    envs:
      - secrets.env # gitignored file

images:
  - name: myapp
    newTag: v1.2.3-rc1

replicas:
  - name: myapp
    count: 2

components:
  - ../../components/monitoring
```

#### k8s/overlays/staging/patch-replicas.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  replicas: 2
  template:
    spec:
      containers:
        - name: app
          resources:
            requests:
              memory: "256Mi"
              cpu: "250m"
            limits:
              memory: "512Mi"
              cpu: "500m"
```

#### k8s/overlays/staging/patch-tolerations.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  template:
    spec:
      tolerations:
        - key: "workload"
          operator: "Equal"
          value: "staging"
          effect: "NoSchedule"
      nodeSelector:
        environment: staging
```

### Production Overlay

#### k8s/overlays/prod/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: myapp-prod

labels:
  - pairs:
      environment: prod
      criticality: high
    includeSelectors: false
    includeTemplates: true

commonAnnotations:
  oncall: sre-team@example.com
  runbook: https://wiki.example.com/myapp-runbook

# Single resources: block. A second resources: key later in the same document
# would silently win (duplicate YAML map keys), dropping ../../base entirely.
resources:
  - ../../base
  - poddisruptionbudget.yaml
  - horizontalpodautoscaler.yaml
  - networkpolicy.yaml

patches:
  - path: patch-replicas.yaml
  - path: patch-resources.yaml
  - path: patch-affinity.yaml
  - path: patch-pdb.yaml
  - path: patch-security.yaml

configMapGenerator:
  - name: app-config
    behavior: merge
    envs:
      - config-values.env

secretGenerator:
  - name: app-secrets
    envs:
      - secrets.env # Managed by external secret management

images:
  - name: myapp
    newTag: v1.2.3
    digest: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855

replicas:
  - name: myapp
    count: 5

components:
  - ../../components/monitoring
  - ../../components/ingress
```

#### k8s/overlays/prod/patch-resources.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  replicas: 5
  template:
    spec:
      containers:
        - name: app
          resources:
            requests:
              memory: "512Mi"
              cpu: "500m"
            limits:
              memory: "1Gi"
              cpu: "1000m"
```

#### k8s/overlays/prod/patch-affinity.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  template:
    spec:
      affinity:
        podAntiAffinity:
          requiredDuringSchedulingIgnoredDuringExecution:
            - labelSelector:
                matchLabels:
                  app: myapp
              topologyKey: kubernetes.io/hostname
        nodeAffinity:
          requiredDuringSchedulingIgnoredDuringExecution:
            nodeSelectorTerms:
              - matchExpressions:
                  - key: workload-type
                    operator: In
                    values:
                      - production
```

#### k8s/overlays/prod/patch-security.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  template:
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: app
          securityContext:
            allowPrivilegeEscalation: false
            capabilities:
              drop:
                - ALL
            readOnlyRootFilesystem: true
          volumeMounts:
            - name: tmp
              mountPath: /tmp
            - name: cache
              mountPath: /app/cache
      volumes:
        - name: tmp
          emptyDir: {}
        - name: cache
          emptyDir: {}
```

#### k8s/overlays/prod/poddisruptionbudget.yaml

```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: myapp-pdb
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: myapp
```

#### k8s/overlays/prod/horizontalpodautoscaler.yaml

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: myapp-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: myapp
  minReplicas: 3
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
    - type: Resource
      resource:
        name: memory
        target:
          type: Utilization
          averageUtilization: 80
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
        - type: Percent
          value: 50
          periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 0
      policies:
        - type: Percent
          value: 100
          periodSeconds: 30
        - type: Pods
          value: 2
          periodSeconds: 30
      selectPolicy: Max
```

#### k8s/overlays/prod/networkpolicy.yaml

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: myapp-netpol
spec:
  podSelector:
    matchLabels:
      app: myapp
  policyTypes:
    - Ingress
    - Egress
  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              name: ingress-nginx
      ports:
        - protocol: TCP
          port: 8080
  egress:
    - to:
        - namespaceSelector:
            matchLabels:
              name: database
      ports:
        - protocol: TCP
          port: 5432
    - to:
        - namespaceSelector:
            matchLabels:
              name: kube-system
          podSelector:
            matchLabels:
              k8s-app: kube-dns
      ports:
        - protocol: UDP
          port: 53
```

### JSON 6902 Patch Example

#### k8s/overlays/prod/kustomization.yaml (excerpt)

```yaml
# Use the unified patches field. patchesStrategicMerge AND patchesJson6902 are
# both deprecated (v5.0.0) and absent from the v1 API; kustomize auto-detects
# the patch type from the file content (a list of {op, path, value} is JSON 6902).
patches:
  - path: json-patch-containers.yaml
    target:
      group: apps
      version: v1
      kind: Deployment
      name: myapp
```

#### k8s/overlays/prod/json-patch-containers.yaml

```yaml
# Add a sidecar container
- op: add
  path: /spec/template/spec/containers/-
  value:
    name: log-shipper
    image: fluent/fluent-bit:2.0
    volumeMounts:
      - name: logs
        mountPath: /var/log/app
        readOnly: true

# Replace the image of the main container (first container)
- op: replace
  path: /spec/template/spec/containers/0/image
  value: registry.example.com/myapp:v1.2.3

# Add environment variable to specific container
- op: add
  path: /spec/template/spec/containers/0/env/-
  value:
    name: FEATURE_FLAG_X
    value: "enabled"

# Remove a specific environment variable (by index)
- op: remove
  path: /spec/template/spec/containers/0/env/3

# Test that a value exists before patching (conditional patch)
- op: test
  path: /spec/replicas
  value: 1
- op: replace
  path: /spec/replicas
  value: 5

# Add a volume
- op: add
  path: /spec/template/spec/volumes/-
  value:
    name: logs
    emptyDir: {}
```

### ConfigMap Generator Examples

#### Literal values

```yaml
configMapGenerator:
  - name: app-config
    literals:
      - LOG_LEVEL=info
      - MAX_RETRIES=3
      - TIMEOUT=30s
```

#### ConfigMap from Files

```yaml
configMapGenerator:
  - name: app-config
    files:
      - application.properties
      - config.json
      - tls.crt=certs/server.crt
```

#### ConfigMap from Env File

```yaml
configMapGenerator:
  - name: app-config
    envs:
      - config.env
```

#### ConfigMap Merging in Overlay

```yaml
configMapGenerator:
  - name: app-config
    behavior: merge # Options: create (default), replace, merge
    literals:
      - LOG_LEVEL=debug # Overrides base value
```

#### Disable name suffix hash

```yaml
configMapGenerator:
  - name: app-config
    options:
      disableNameSuffixHash: true
    literals:
      - KEY=value
```

### Secret Generator Examples

#### Secret from Literals

```yaml
secretGenerator:
  - name: app-secrets
    literals:
      - username=admin
      - password=changeme
```

#### Secret from Files

```yaml
secretGenerator:
  - name: tls-secrets
    files:
      - tls.crt
      - tls.key
    type: kubernetes.io/tls
```

#### Secret from Env File (Gitignored)

```yaml
secretGenerator:
  - name: app-secrets
    envs:
      - secrets.env # File not committed to git
```

### Component Example: Monitoring

#### k8s/components/monitoring/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1alpha1
kind: Component

resources:
  - servicemonitor.yaml
  - prometheusrule.yaml

patches:
  - path: patch-metrics.yaml
    target:
      kind: Deployment

labels:
  - pairs:
      prometheus.io/scrape: "true"
    includeSelectors: false
```

#### k8s/components/monitoring/servicemonitor.yaml

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: myapp
spec:
  selector:
    matchLabels:
      app: myapp
  endpoints:
    - port: metrics
      interval: 30s
      path: /metrics
```

#### k8s/components/monitoring/prometheusrule.yaml

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: myapp-alerts
spec:
  groups:
    - name: myapp
      interval: 30s
      rules:
        - alert: HighErrorRate
          expr: |
            rate(http_requests_total{status=~"5.."}[5m]) > 0.05
          for: 5m
          labels:
            severity: warning
          annotations:
            summary: High error rate detected
            description: Error rate is {{ $value }} req/s
```

#### k8s/components/monitoring/patch-metrics.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: not-important
spec:
  template:
    spec:
      containers:
        - name: app
          ports:
            - containerPort: 9090
              name: metrics
              protocol: TCP
```

### Component Example: Ingress

#### k8s/components/ingress/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1alpha1
kind: Component

resources:
  - ingress.yaml

patches:
  - path: patch-service.yaml
    target:
      kind: Service
```

#### k8s/components/ingress/ingress.yaml

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: myapp
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/rate-limit: "100"
spec:
  ingressClassName: nginx
  tls:
    - hosts:
        - myapp.example.com
      secretName: myapp-tls
  rules:
    - host: myapp.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: myapp
                port:
                  name: http
```

#### k8s/components/ingress/patch-service.yaml

```yaml
apiVersion: v1
kind: Service
metadata:
  name: not-important
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-backend-protocol: http
```

### Transformers Example

#### Using built-in transformers

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

# Add prefix to all resource names
namePrefix: myapp-

# Add suffix to all resource names
nameSuffix: -v2

# Set namespace for all resources
namespace: production

# Add labels to all resources
commonLabels:
  app: myapp
  team: platform
  environment: prod

# Add annotations to all resources
commonAnnotations:
  managed-by: kustomize
  contact: team@example.com

# Transform images
images:
  - name: nginx
    newName: my-registry/nginx
    newTag: 1.21.0
  - name: redis
    newName: my-registry/redis
    digest: sha256:a4d4e6f8c9b0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6

# Set replicas for deployments
replicas:
  - name: myapp
    count: 3
  - name: worker
    count: 2

# Add labels to specific resources
labels:
  - pairs:
      version: v2
    includeSelectors: true
    includeTemplates: true

# Transform resource names/namespaces
replacements:
  - source:
      kind: ConfigMap
      name: app-config
      fieldPath: metadata.name
    targets:
      - select:
          kind: Deployment
        fieldPaths:
          - spec.template.spec.volumes.[name=config].configMap.name
```

### Advanced: Using Replacements for Dynamic References

#### Replacements Base Configuration

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  - deployment.yaml
  - service.yaml
  - configmap.yaml

replacements:
  # Replace service name in ingress based on actual service name
  - source:
      kind: Service
      name: myapp
      fieldPath: metadata.name
    targets:
      - select:
          kind: Ingress
        fieldPaths:
          - spec.rules.[host=myapp.example.com].http.paths.[path=/].backend.service.name

  # Propagate ConfigMap name to Deployment (handles hash suffix)
  - source:
      kind: ConfigMap
      name: app-config
      fieldPath: metadata.name
    targets:
      - select:
          kind: Deployment
        fieldPaths:
          - spec.template.spec.volumes.[name=config].configMap.name
```

### Using Remote Bases

#### Remote Base Reference

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  # GitHub repo
  - https://github.com/org/repo/k8s/base?ref=v1.0.0

  # Specific path in repo
  - github.com/org/repo/manifests?ref=main

patches:
  - path: local-patch.yaml
```

### Multi-Environment with Shared Component

#### Shared Component Base Configuration

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  - deployment.yaml
  - service.yaml
```

#### Shared Component Dev Overlay

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: dev

resources:
  - ../../base

components:
  - ../../components/debug-tools

replicas:
  - name: myapp
    count: 1
```

#### Shared Component Prod Overlay

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: prod

resources:
  - ../../base

components:
  - ../../components/monitoring
  - ../../components/ingress

replicas:
  - name: myapp
    count: 5
```

#### k8s/components/debug-tools/kustomization.yaml

```yaml
apiVersion: kustomize.config.k8s.io/v1alpha1
kind: Component

patches:
  - path: patch-debug.yaml

configMapGenerator:
  - name: app-config
    behavior: merge
    literals:
      - DEBUG=true
      - ENABLE_PPROF=true
```

## Common Tasks

### Validate Kustomization

```bash
# Build and validate
kustomize build k8s/overlays/prod

# Use kubectl (includes additional validation)
kubectl kustomize k8s/overlays/prod

# Validate against cluster without applying
kubectl apply -k k8s/overlays/prod --dry-run=server

# Check diff before applying
kubectl diff -k k8s/overlays/prod
```

### Extract Common Configuration

When you notice duplication across overlays:

1. Identify common patches or resources
2. Move to base or create a component
3. Reference from overlays

```bash
# Before: Same patch in dev, staging, prod
# After: Move to component
mkdir -p k8s/components/common-settings
# Create component kustomization
# Reference from each overlay
```

### Debug Name Transformations

```bash
# See final resource names after transformations
kustomize build k8s/overlays/prod | grep "^  name:"

# Check ConfigMap/Secret name with hash
kustomize build k8s/overlays/prod | grep -A 2 "kind: ConfigMap"
```

### Convert Existing Manifests

```bash
# Generate kustomization.yaml from existing resources
cd k8s/base
kustomize create --autodetect

# Or manually specify
kustomize create --resources deployment.yaml,service.yaml
```

### Update Image Tags

```bash
# Update image tag in kustomization.yaml
cd k8s/overlays/prod
kustomize edit set image myapp=registry.example.com/myapp:v1.3.0

# Or use kubectl
kubectl set image deployment/myapp myapp=registry.example.com/myapp:v1.3.0 --dry-run=client -o yaml | kubectl apply -k .
```

### Add Resources

```bash
cd k8s/base
kustomize edit add resource new-deployment.yaml
```

### Add ConfigMap Generator

```bash
cd k8s/overlays/dev
kustomize edit add configmap app-config --from-literal=KEY=value
```

## Integration Patterns

### GitOps with ArgoCD

#### argocd-application.yaml

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-prod
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/org/repo
    targetRevision: main
    path: k8s/overlays/prod
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp-prod
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
      - CreateNamespace=true
```

### GitOps with Flux

#### kustomization.yaml

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: myapp-prod
  namespace: flux-system
spec:
  interval: 10m
  path: ./k8s/overlays/prod
  prune: true
  sourceRef:
    kind: GitRepository
    name: myapp
  validation: client
  healthChecks:
    - apiVersion: apps/v1
      kind: Deployment
      name: myapp
      namespace: myapp-prod
```

### CI/CD Pipeline

```bash
#!/bin/bash
# build-and-validate.sh

set -euo pipefail

OVERLAY=${1:-dev}
OUTPUT_DIR="manifests/${OVERLAY}"

# Build manifests
kustomize build "k8s/overlays/${OVERLAY}" > "${OUTPUT_DIR}/all.yaml"

# Validate with kubeconform (kubeval is unmaintained, stale schemas)
kubeconform -strict -summary -kubernetes-version 1.29.0 "${OUTPUT_DIR}/all.yaml"

# Validate with kube-score
kube-score score "${OUTPUT_DIR}/all.yaml"

# Policy validation with OPA/Conftest
conftest test "${OUTPUT_DIR}/all.yaml"

# Commit rendered manifests for GitOps
git add "${OUTPUT_DIR}/all.yaml"
```

### Helm Integration

> **`helmCharts` requires `kustomize build --enable-helm`.** Without the flag the entire `helmCharts` section is silently skipped — no error, no warning — so CI can emit manifests missing whole components yet still "pass". `kubectl apply -k` CANNOT pass `--enable-helm` (returns `unknown flag: --enable-helm`), so `helmCharts` is effectively dead there — use `kustomize build --enable-helm | kubectl apply -f -` (or `kubectl kustomize --enable-helm`). The feature is also a deliberately limited subset: private-registry auth and post-renderers are explicitly unsupported. For anything beyond simple inflation, render with `helm template` and apply kustomize patches on top.

```yaml
# Use kustomize to customize Helm output. Build with:
#   kustomize build --enable-helm k8s/overlays/prod | kubectl apply -f -
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

helmCharts:
  - name: postgresql
    repo: https://charts.bitnami.com/bitnami
    version: 12.1.2
    releaseName: myapp-db
    namespace: database
    valuesInline:
      auth:
        username: myapp
        database: myapp_prod

patches:
  - path: patch-postgresql.yaml
    target:
      kind: StatefulSet
      name: myapp-db-postgresql
```

## Troubleshooting

### Common Errors

#### Error: Accumulating Resources

```text
accumulating resources: accumulation err='accumulating resources from '../../base':
evalsymlink failure on '/path/to/base' : lstat /path/to/base: no such file or directory'
```

- **Solution**: Check that base path in overlay's kustomization.yaml is correct
- Paths are relative to the kustomization.yaml location

#### Error: No Matches for OriginalId

```text
no matches for OriginalId ~G_v1_ConfigMap|~X|app-config;
failed to find unique target for patch
```

- **Solution**: Ensure the resource being patched exists in base
- Check resource name and kind match exactly

#### Error: Conflict Between Patches

```text
conflict: multiple matches for ...
```

- **Solution**: Make patches more specific with metadata
- Use JSON patch for precise targeting

#### Error: Cyclic Dependency

```text
base 'overlays/dev' refers to base '../../base' which refers back to 'overlays/dev'
```

- **Solution**: Check for circular references in bases
- Bases should not reference overlays

### Debugging Techniques

```bash
# Build with the default (safe) load restrictor
kustomize build k8s/overlays/prod

# Show resources before and after transformation
kustomize build k8s/base > base.yaml
kustomize build k8s/overlays/prod > overlay.yaml
diff base.yaml overlay.yaml

# Validate specific resource
kustomize build k8s/overlays/prod | kubectl apply --dry-run=client -f -

# Check for YAML syntax errors
kustomize build k8s/overlays/prod | yamllint -

# Inspect ConfigMap hash generation
kustomize build k8s/overlays/prod | grep -A 10 "kind: ConfigMap"
```

## Performance Optimization

### Large-Scale Kustomizations

1. **Use components for modularity**: Break large kustomizations into components
2. **Avoid deep overlay chains**: Keep hierarchy shallow (base -> overlay, not base -> overlay1 -> overlay2)
3. **Cache remote bases**: Use local copies for frequently referenced remote bases
4. **Parallelize builds**: Build multiple overlays in parallel in CI
5. **Limit resource scope**: Don't kustomize resources that don't need customization

### Build Time Optimization

```bash
# Build with the default (safe) root-only load restrictor
kustomize build k8s/overlays/prod

# Build multiple environments in parallel
parallel kustomize build k8s/overlays/{} ::: dev staging prod
```

> **Do not reach for `--load-restrictor=LoadRestrictionsNone` to "fix" cross-directory file loading.** It disables Kustomize's file-access sandbox (see Security Considerations). The correct fix for genuine shared-file needs is to create a base with its own `kustomization.yaml` and reference it.

## Security Considerations

1. **Secret Management**: Never commit secrets to git
   - Use external secret operators (sealed-secrets, external-secrets-operator)
   - Use secret generators with gitignored files
   - Consider using SOPS for encrypted secrets in git

2. **Image Security**: Pin images by digest in production

   ```yaml
   images:
     - name: myapp
       digest: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
   ```

3. **RBAC**: Separate access to base vs overlays
   - Base: Restricted to platform team
   - Overlays: Application teams can customize

4. **Validation**: Use admission controllers
   - OPA Gatekeeper
   - Kyverno policies
   - Custom admission webhooks

5. **Audit**: Track kustomization changes
   - Git commit history
   - CI/CD logs
   - Kubernetes audit logs

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance for production Kustomize. Most of these failure modes are **silent** — wrong output with exit code 0 — so always diff `kustomize build` output before and after any structural change.

### Anti-Patterns (deprecated fields removed from the v1 API)

**`commonLabels` → `labels` transformer.** `commonLabels` (deprecated v5.0.0) injects every label into `spec.selector.matchLabels` in addition to `metadata.labels` and pod templates. Because a Deployment/StatefulSet/Service selector is **immutable after first apply**, adding or changing any `commonLabel` later forces a delete-and-recreate. The `labels` transformer defaults `includeSelectors: false` (safe on live resources); add `includeTemplates: true` so pod labels still propagate (Prometheus scraping, log aggregation). Reserve `includeSelectors: true` for stable identity labels set only at creation time — and prefer defining those directly in base manifests so the selector contract is explicit. The docs warn that changing selectors on live resources "could result in failures."

```yaml
labels:
  - pairs:
      app.kubernetes.io/part-of: myplatform
      environment: prod
    includeSelectors: false   # safe on live resources — does NOT touch matchLabels
    includeTemplates: true    # still reaches pod template labels
```

**`patchesStrategicMerge` / `patchesJson6902` → unified `patches`.** Both deprecated in v5.0.0. The `patches` field auto-detects the type from content (resource-shaped doc = strategic merge; list of `{op, path, value}` = JSON 6902), and its `target` supports `group/version/kind/name/namespace` plus `labelSelector` and `annotationSelector`, so one patch can hit many resources. **Migration is not behavior-neutral:** `patches` runs *after* the label transformers while `patchesJson6902` ran *before* them, so a patch that touched `/metadata/labels` can produce different output post-migration. Always diff.

**`vars` → `replacements`.** `vars` (`$(VAR)` substitution, deprecated v5.0.0) is replaced by `replacements`, a structured source/target model that propagates any field value (including hashed ConfigMap/Secret names). **Migration is NOT 1:1:** `replacements` works on field pointers and cannot do free-form mid-string concatenation like `path: $(ROOT)/cluster/$(REVISION)`. It *can* do single-segment replacement within a string via the `delimiter` + `index` options, but for true composite strings, restructure (compose in an init container/env var) before migrating.

```yaml
replacements:
  - source: {kind: ConfigMap, name: app-config, fieldPath: metadata.name}
    targets:
      - select: {kind: Deployment}
        fieldPaths:
          - spec.template.spec.volumes.[name=config].configMap.name
```

### Idioms

**`kustomize edit fix` is the canonical, idempotent migration tool.** It migrates deprecated fields in place — `bases`→`resources`, `commonLabels`→`labels`, `patchesStrategicMerge`/`patchesJson6902`→`patches`, and (experimentally, `--vars`) `vars`→`replacements`. Run it per kustomization directory; use it as a CI lint to surface remaining deprecated fields. Two things it can't guarantee semantically: the patch apply-order shift and `vars` mid-string interpolation — so wrap it in a build diff:

```bash
kustomize build . > before.yaml
kustomize edit fix
kustomize build . > after.yaml
diff before.yaml after.yaml   # confirm semantic equivalence
```

**Components apply against the accumulated resource set.** Components use `apiVersion: kustomize.config.k8s.io/v1alpha1`, `kind: Component` (alpha — the API may change without a deprecation window). Unlike a base, a Component runs its patches/generators against the resource set the *parent* has accumulated so far, enabling additive opt-in feature bundles. They must be referenced under `components:`, never `resources:`, and **can be nested** (a Component may itself reference other Components via `components:`).

**Design with Kustomize's deliberate constraints.** The maintainers commit to NOT supporting: (1) removal directives (you cannot delete a label/patch/resource from a base — restructure/fork the base, or use a JSON 6902 `op: remove` for a specific path); (2) `${VAR}` templating; (3) build-time side effects from CLI args or shell env vars (output must be fully determined by version-controlled files); (4) globs. The sanctioned pattern for env-driven output is to mutate the kustomization, commit, then build:

```bash
kustomize edit set image myapp=registry.example.com/myapp:${GIT_SHA}
kustomize build overlays/prod
```

### Gotchas (silent failures)

**Strategic merge patches silently REPLACE entire list fields on CRDs.** For built-in types, strategic merge knows array merge keys from the embedded OpenAPI schema (e.g. `containers` merges on `name`). Custom Resources have no such schema, so kustomize falls back to JSON merge semantics: a patched list field **replaces the whole array** with no error — dropping every element not in the patch. Fix with either an `openapi` field carrying `x-kubernetes-patch-merge-key`/`x-kubernetes-patch-strategy`, or a JSON 6902 patch (which behaves identically for built-in and custom resources):

```yaml
patches:
  - target: {group: argoproj.io, version: v1alpha1, kind: Application, name: my-app}
    patch: |-
      - op: add
        path: /spec/sources/-
        value:
          repoURL: https://github.com/org/repo
```

**`disableNameSuffixHash: true` defeats config-driven rolling updates — and a global `true` cannot be overridden.** The content-hash suffix (`app-config-k8bgk8h4t6`) is what triggers a rollout on config change: the new name changes the pod spec, so pods are replaced. Disabling it gives a stable name but silently breaks this — config changes apply to the ConfigMap, but Deployments do not roll. Worse, the boolean follows "global true wins": setting it in top-level `generatorOptions` strips the hash from **every** generator, and a per-generator `disableNameSuffixHash: false` is **ignored**. Only ever disable per-generator, and only for resources referenced by a fixed external name; if you need a stable name *and* rolling updates, keep the hash and propagate the hashed name via `replacements`.

**`replacements` list filters like `[name=nginx]` are unanchored regex.** Since v4.5.3, `spec.template.spec.containers.[name=nginx].image` matches *any* element whose name *contains* `nginx` — `my-nginx`, `nginx-sidecar` — and all matches are targeted, silently overwriting several containers. Anchor explicitly with `[name=^nginx$]`, or use index-based paths. (This differs from a `patches` `target`'s `name`/`namespace`, which ARE auto-anchored.)

**`configMapGenerator` `behavior: merge` matches by logical name AND namespace.** An overlay generator with `behavior: merge` matches the base by pre-hash name — but if one side specifies `namespace` and the other omits it, kustomize treats them as different generators and creates a **second** ConfigMap instead of merging (no error). `merge` also only merges discrete keys; if both reference the same filename, the overlay file replaces the base file entirely. Keep `namespace` consistent (or absent) on both sides, and verify the build emits one ConfigMap, not two.

**`sortOptions` only applies to the top-level (build-target) kustomization.** `sortOptions` (`order: legacy | fifo`; v5.0.0+, replaces `--reorder`) controls output ordering — `legacy` (default) places Namespaces/ServiceAccounts/RBAC first, which is why `kubectl apply -k` usually works without explicit ordering. Instances in bases/components reached via `resources` are **ignored**, so set it in the overlay that is the actual build target (what ArgoCD/Flux points to), never in a shared base.

**Pin a standalone kustomize version.** kubectl embeds an older kustomize that does not track standalone releases (the lag has spanned major versions). Features added after the embedded version — unified `patches`, `sortOptions`, Components, newer `replacements` behaviors — may error or be silently ignored under `kubectl apply -k` while a dev's standalone v5 produces correct output. ArgoCD/Flux also bundle their own versions. Standardize on a pinned `kustomize build | kubectl apply -f -` in CI and match the GitOps controller's version. Check with `kustomize version` and `kubectl version --client -o json` (`kustomizeVersion`).

### Security

**Never use `--load-restrictor=LoadRestrictionsNone` with untrusted input.** The default `LoadRestrictionsRootOnly` (v2.0) prevents file references (`resources`, `configMapGenerator` files) from reading outside the kustomization root — a real security boundary. With it off, a kustomization you don't fully control (e.g. a compromised remote base) can use a `configMapGenerator` file reference to read arbitrary host files (credentials, TLS keys, `/etc/passwd`). It also breaks relocatability. Only use it in fully trusted, audited build environments; the legitimate alternative for cross-directory file needs is a proper base with its own `kustomization.yaml`.

**Pin remote bases to a full commit SHA or tag.** Kustomize fetches remote bases over the network at build time. `?ref=main` (or no ref, which uses default-branch HEAD) means a force-push or compromised upstream flows straight into your cluster on the next reconcile with no change to your repo. Short SHAs are unsupported — use a full fetchable SHA (strongest; tags can be force-pushed) or a versioned tag. Remote fetches also add latency and a rate-limit/availability dependency, so mirror critical bases locally. In GitOps, prefer the controller's own authenticated `GitRepository`/`OCIRepository` source.

```yaml
resources:
  - https://github.com/org/platform-base//k8s/base?ref=a3f8c2d1e4b5f6a7b8c9d0e1f2a3b4c5d6e7f8a9   # full SHA
  - https://github.com/org/platform-base//k8s/base?ref=v2.3.1                                       # or versioned tag
```

### Currency

**Validate with `kubeconform`, not `kubeval`.** `kubeval` is unmaintained and its schema registry is stale, so it cannot validate recent Kubernetes API versions and silently passes manifests using newer/renamed fields. `kubeconform` is the maintained successor on the same JSON-schema approach, with a current registry and CRD support via `-schema-location`:

```bash
kustomize build k8s/overlays/prod | kubeconform -strict -summary -kubernetes-version 1.29.0
```

## Summary

Kustomize provides a declarative, Kubernetes-native approach to configuration management:

- **Use bases** for shared, environment-agnostic configuration
- **Use overlays** for environment-specific customization
- **Use components** for optional, reusable features
- **Use generators** for ConfigMaps and Secrets with content hashing
- **Use transformers** for cross-cutting modifications
- **Prefer strategic merge** for simplicity, JSON patch for precision
- **Keep structure shallow** to avoid complexity
- **Validate early** with `kustomize build` and `kubectl diff`
- **Secure secrets** with external tools, never commit sensitive data
- **Document patterns** so team members understand customization strategy

Kustomize integrates seamlessly with kubectl, GitOps tools (ArgoCD, Flux), and CI/CD pipelines, making it an excellent choice for Kubernetes configuration management.
