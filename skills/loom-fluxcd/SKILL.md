---
name: loom-fluxcd
description: GitOps continuous delivery toolkit for Kubernetes with Flux CD. Use for declarative deployments, Helm chart automation, Kustomize overlays, image update automation, multi-tenancy, and Git-based continuous delivery.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - flux
  - fluxcd
  - gitops
  - kustomization
  - helmrelease
  - gitrepository
  - helmrepository
  - imagerepository
  - imagepolicy
  - image automation
  - source controller
  - continuous delivery
  - kubernetes deployment automation
  - helm automation
  - kustomize automation
  - git sync
  - declarative deployment
---

# Flux CD GitOps Toolkit

## Overview

Flux CD is a declarative, GitOps continuous delivery solution for Kubernetes. It automatically ensures that the state of your Kubernetes cluster matches the configuration stored in Git repositories.

**When to use this skill:**

- Implementing GitOps workflows for Kubernetes
- Automating Helm chart deployments and upgrades
- Managing Kustomize overlays across environments
- Automating container image updates from registries
- Setting up multi-tenant Kubernetes with isolated teams
- Integrating Git-based continuous delivery pipelines
- Managing infrastructure and application dependencies
- Implementing progressive delivery with canary deployments

### Core Architecture

Flux is composed of specialized controllers, each handling specific aspects of GitOps:

#### Source Controller

- **GitRepository**: Fetches artifacts from Git repositories
- **HelmRepository**: Fetches Helm charts from chart repositories
- **HelmChart**: Fetches charts from GitRepository or HelmRepository sources
- **Bucket**: Fetches artifacts from S3-compatible storage

#### Kustomize Controller

- **Kustomization**: Applies Kustomize overlays and manages reconciliation
- Supports dependency ordering and health checks
- Handles pruning of deleted resources

#### Helm Controller

- **HelmRelease**: Manages Helm chart installations and upgrades
- Supports automated remediation and testing
- Handles rollbacks on failure

#### Notification Controller

- **Provider**: Defines notification endpoints (Slack, MS Teams, etc.)
- **Alert**: Sends alerts based on resource events
- **Receiver**: Handles webhook notifications from external systems

#### Image Automation Controllers

- **ImageRepository**: Scans container registries for image metadata
- **ImagePolicy**: Defines rules for selecting image tags
- **ImageUpdateAutomation**: Updates Git repository with new image tags

## Installation and Bootstrap

### Prerequisites

```bash

# Install Flux CLI
curl -s https://fluxcd.io/install.sh | sudo bash

# Or using Homebrew
brew install fluxcd/tap/flux

# Verify installation
flux --version
```

### Bootstrap with GitHub

```bash
# Export GitHub personal access token
export GITHUB_TOKEN=<your-token>

# Bootstrap Flux
flux bootstrap github \
  --owner=<github-username> \
  --repository=<repo-name> \
  --branch=main \
  --path=clusters/production \
  --personal \
  --components-extra=image-reflector-controller,image-automation-controller
```

### Bootstrap with GitLab

```bash
export GITLAB_TOKEN=<your-token>

flux bootstrap gitlab \
  --owner=<gitlab-group> \
  --repository=<repo-name> \
  --branch=main \
  --path=clusters/production \
  --personal
```

### Pre-commit Validation

Check your manifests before committing:

```bash
# Validate all Flux resources
flux check

# Check specific resources
kubectl apply --dry-run=server -f clusters/production/
```

## Repository Structure Best Practices

### Standard Layout

```text
├── clusters/
│   ├── production/
│   │   ├── flux-system/           # Flux components (managed by bootstrap)
│   │   ├── infrastructure.yaml    # Infrastructure sources & kustomizations
│   │   └── apps.yaml              # Application sources & kustomizations
│   └── staging/
│       ├── flux-system/
│       ├── infrastructure.yaml
│       └── apps.yaml
├── infrastructure/
│   ├── base/                      # Base infrastructure
│   │   ├── ingress-nginx/
│   │   ├── cert-manager/
│   │   └── sealed-secrets/
│   └── overlays/
│       ├── production/
│       └── staging/
└── apps/
    ├── base/
    │   ├── app1/
    │   └── app2/
    └── overlays/
        ├── production/
        └── staging/
```

### Multi-Tenancy Layout

```text
├── clusters/
│   └── production/
│       ├── flux-system/
│       ├── tenants/
│       │   ├── team-a.yaml        # Team A namespace and RBAC
│       │   └── team-b.yaml        # Team B namespace and RBAC
│       └── infrastructure.yaml
├── tenants/
│   ├── base/
│   │   ├── team-a/
│   │   │   ├── namespace.yaml
│   │   │   ├── rbac.yaml
│   │   │   └── sync.yaml          # GitRepository + Kustomization for team
│   │   └── team-b/
│   │       ├── namespace.yaml
│   │       ├── rbac.yaml
│   │       └── sync.yaml
│   └── overlays/
│       └── production/
└── teams/                         # Separate repos or paths for each team
    ├── team-a-repo/
    └── team-b-repo/
```

## GitRepository and Kustomization

### Basic GitRepository

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata:
  name: flux-system
  namespace: flux-system
spec:
  interval: 1m0s
  ref:
    branch: main
  url: https://github.com/org/repo
  secretRef:
    name: flux-system
```

### GitRepository with Specific Path

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata:
  name: apps
  namespace: flux-system
spec:
  interval: 5m0s
  ref:
    branch: main
  url: https://github.com/org/apps-repo
  ignore: |
    # Exclude all
    /*
    # Include specific paths
    !/apps/production/
```

### Basic Kustomization

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: infrastructure
  namespace: flux-system
spec:
  interval: 10m0s
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./infrastructure/production
  prune: true
  wait: true
  timeout: 5m0s
```

### Kustomization with Dependencies

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: apps
  namespace: flux-system
spec:
  interval: 10m0s
  dependsOn:
    - name: infrastructure
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./apps/production
  prune: true
  wait: true
  timeout: 5m0s
  healthChecks:
    - apiVersion: apps/v1
      kind: Deployment
      name: app-name
      namespace: app-namespace
  postBuild:
    substitute:
      cluster_name: production
      domain: example.com
    substituteFrom:
      - kind: ConfigMap
        name: cluster-vars
```

### Variable Substitution

Create a ConfigMap for cluster-specific variables:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: cluster-vars
  namespace: flux-system
data:
  cluster_name: production
  cluster_region: us-east-1
  domain: example.com
```

Use variables in manifests:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: app-config
  namespace: default
data:
  cluster: ${cluster_name}
  region: ${cluster_region}
  url: https://app.${domain}
```

## Multi-Tenancy Patterns

### Namespace Isolation

Flux supports multi-tenant clusters where teams have isolated namespaces with their own GitRepository sources and Kustomizations.

### Tenant Bootstrap Pattern

```yaml
# clusters/production/tenants/team-a.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: team-a
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: team-a-reconciler
  namespace: team-a
---
# Namespace-scoped RoleBinding to the built-in `admin` ClusterRole.
# This grants full rights WITHIN team-a only — never use a ClusterRoleBinding
# and never bind a tenant reconciler to cluster-admin (see Tenant RBAC Restrictions).
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: team-a-reconciler
  namespace: team-a
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: admin
subjects:
  - kind: ServiceAccount
    name: team-a-reconciler
    namespace: team-a
---
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata:
  name: team-a-repo
  namespace: team-a
spec:
  interval: 1m
  url: https://github.com/org/team-a-repo
  ref:
    branch: main
---
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: team-a-apps
  namespace: team-a
spec:
  interval: 10m
  serviceAccountName: team-a-reconciler
  sourceRef:
    kind: GitRepository
    name: team-a-repo
  path: ./apps
  prune: true
```

### Tenant RBAC Restrictions

Restrict tenant reconcilers to their namespace only:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: team-a-reconciler
  namespace: team-a
rules:
  - apiGroups: ["*"]
    resources: ["*"]
    verbs: ["*"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: team-a-reconciler
  namespace: team-a
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: Role
  name: team-a-reconciler
subjects:
  - kind: ServiceAccount
    name: team-a-reconciler
    namespace: team-a
```

### Multi-Tenancy Lockdown Flags (mandatory)

RBAC alone does NOT make a Flux install multi-tenant. A default install lets a
tenant reference Sources/Secrets in other namespaces, pull arbitrary remote
Kustomize bases, and (if it omits `serviceAccountName`) reconcile with the
controller's cluster-wide identity. Three controller flags close these vectors
and MUST be set via bootstrap kustomize patches in
`clusters/<env>/flux-system/`:

- `--no-cross-namespace-refs=true` on kustomize/helm/notification/image-reflector/image-automation controllers — blocks cross-namespace references to Sources, Secrets, and events.
- `--no-remote-bases=true` on kustomize-controller — blocks fetching arbitrary Kustomize bases over HTTPS (which bypass Flux source verification and caching).
- `--default-service-account=default` on kustomize/helm controllers — any resource lacking `spec.serviceAccountName` falls back to the namespace `default` SA (which should have no RBAC) instead of the controller identity.

```yaml
# clusters/<env>/flux-system/kustomization.yaml — patches the bootstrapped components
patches:
  - patch: |
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --no-cross-namespace-refs=true
    target:
      kind: Deployment
      name: "(kustomize-controller|helm-controller|notification-controller|image-reflector-controller|image-automation-controller)"
  - patch: |
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --no-remote-bases=true
    target:
      kind: Deployment
      name: kustomize-controller
  - patch: |
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --default-service-account=default
    target:
      kind: Deployment
      name: "(kustomize-controller|helm-controller)"
```

With these set, every tenant Kustomization/HelmRelease MUST declare
`spec.serviceAccountName` (bound to a namespace-scoped Role or the built-in
`admin` ClusterRole via RoleBinding); a resource that forgets it inherits the
powerless `default` SA rather than escalating.

### Cross-Tenant Dependencies

Teams can depend on shared infrastructure while maintaining isolation:

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: team-a-apps
  namespace: team-a
spec:
  interval: 10m
  dependsOn:
    - name: shared-ingress
      namespace: flux-system
    - name: shared-monitoring
      namespace: flux-system
  sourceRef:
    kind: GitRepository
    name: team-a-repo
  path: ./apps
  prune: true
```

## Helm Integration

Flux provides deep integration with Helm for chart-based deployments.

### Helm Repository and Helm Release

### HelmRepository

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: HelmRepository
metadata:
  name: bitnami
  namespace: flux-system
spec:
  interval: 1h0s
  url: https://charts.bitnami.com/bitnami
```

### HelmRepository with Authentication

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: HelmRepository
metadata:
  name: private-charts
  namespace: flux-system
spec:
  interval: 1h0s
  url: https://charts.example.com
  secretRef:
    name: helm-charts-auth
---
apiVersion: v1
kind: Secret
metadata:
  name: helm-charts-auth
  namespace: flux-system
type: Opaque
stringData:
  username: user
  password: pass
```

### Basic HelmRelease

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: nginx-ingress
  namespace: ingress-nginx
spec:
  interval: 10m0s
  chart:
    spec:
      chart: ingress-nginx
      version: "4.8.x"
      sourceRef:
        kind: HelmRepository
        name: ingress-nginx
        namespace: flux-system
      interval: 1h0s
  values:
    controller:
      service:
        type: LoadBalancer
```

### HelmRelease with ValuesFrom

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: my-app
  namespace: apps
spec:
  interval: 10m0s
  chart:
    spec:
      chart: my-app
      version: "1.0.x"
      sourceRef:
        kind: HelmRepository
        name: my-charts
        namespace: flux-system
  values:
    replicas: 2
  valuesFrom:
    - kind: ConfigMap
      name: app-config
      valuesKey: values.yaml
    - kind: Secret
      name: app-secrets
      valuesKey: secrets.yaml
```

### HelmRelease with Testing and Rollback

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: my-app
  namespace: apps
spec:
  interval: 10m0s
  chart:
    spec:
      chart: my-app
      version: "1.0.x"
      sourceRef:
        kind: HelmRepository
        name: my-charts
        namespace: flux-system
  install:
    remediation:
      retries: 3
  upgrade:
    remediation:
      retries: 3
      remediateLastFailure: true
    cleanupOnFail: true
  test:
    enable: true
  rollback:
    cleanupOnFail: true
    recreate: true
  values:
    image:
      tag: v1.0.0
```

### HelmRelease with Dependencies

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: my-app
  namespace: apps
spec:
  interval: 10m0s
  dependsOn:
    - name: cert-manager
      namespace: cert-manager
    - name: nginx-ingress
      namespace: ingress-nginx
  chart:
    spec:
      chart: my-app
      version: "1.0.x"
      sourceRef:
        kind: HelmRepository
        name: my-charts
        namespace: flux-system
  values:
    ingress:
      enabled: true
      className: nginx
```

## Secret Management with SOPS

### Install SOPS and Age

```bash
# Install SOPS
brew install sops

# Install Age
brew install age

# Generate Age key
age-keygen -o age.agekey

# Get public key for .sops.yaml
age-keygen -y age.agekey
```

### Configure SOPS

Create `.sops.yaml` in repository root:

```yaml
creation_rules:
  - path_regex: .*/production/.*\.yaml
    encrypted_regex: ^(data|stringData)$
    age: age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p
  - path_regex: .*/staging/.*\.yaml
    encrypted_regex: ^(data|stringData)$
    age: age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p
```

### Create Encrypted Secret

```bash
# Create secret manifest
cat <<EOF > secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: app-secrets
  namespace: apps
stringData:
  username: admin
  password: supersecret
EOF

# Encrypt with SOPS
sops --encrypt --in-place secret.yaml

# Decrypt for viewing
sops --decrypt secret.yaml
```

### Configure Flux for SOPS Decryption

Create secret with Age private key:

```bash
cat age.agekey | kubectl create secret generic sops-age \
  --namespace=flux-system \
  --from-file=age.agekey=/dev/stdin
```

Configure Kustomization to decrypt:

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: apps
  namespace: flux-system
spec:
  interval: 10m0s
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./apps/production
  prune: true
  decryption:
    provider: sops
    secretRef:
      name: sops-age
```

### SOPS with Multiple Keys

For team collaboration, add multiple Age keys:

```yaml
creation_rules:
  - path_regex: .*/production/.*\.yaml
    encrypted_regex: ^(data|stringData)$
    age: >-
      age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p,
      age1zvkyg2lqzraa2lnjvqej32nkuu0ues2s82hzrye869xeexvn73equnujwj,
      age1penhr3v0pklzv6lqrvt3zyqhfvqffkjn5j2qhzc8xr7q8vpfck4q7n8k3f
```

## Image Automation

Flux can automatically detect new container image versions and update manifests in Git.

### Image Automation Architecture

The image automation workflow consists of three resources:

1. **ImageRepository** - Scans container registry for available tags
2. **ImagePolicy** - Defines tag selection rules (semver, regex, alphabetical)
3. **ImageUpdateAutomation** - Commits updated image tags back to Git

### Image Automation Workflow

```text
Container Registry
       |
       | (scan for tags)
       v
ImageRepository
       |
       | (filter & select)
       v
  ImagePolicy
       |
       | (update manifests)
       v
ImageUpdateAutomation
       |
       | (commit to Git)
       v
   GitRepository
       |
       | (reconcile)
       v
  Kustomization
       |
       v
   Kubernetes Cluster
```

### ImageRepository

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageRepository
metadata:
  name: my-app
  namespace: flux-system
spec:
  image: ghcr.io/org/my-app
  interval: 1m0s
```

### ImageRepository with Authentication

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageRepository
metadata:
  name: my-app
  namespace: flux-system
spec:
  image: registry.example.com/org/my-app
  interval: 1m0s
  secretRef:
    name: registry-credentials
---
apiVersion: v1
kind: Secret
metadata:
  name: registry-credentials
  namespace: flux-system
type: kubernetes.io/dockerconfigjson
data:
  .dockerconfigjson: <base64-encoded-docker-config>
```

### ImagePolicy - Semantic Versioning

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImagePolicy
metadata:
  name: my-app
  namespace: flux-system
spec:
  imageRepositoryRef:
    name: my-app
  policy:
    semver:
      range: 1.0.x
```

### ImagePolicy - Alphabetical

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImagePolicy
metadata:
  name: my-app-develop
  namespace: flux-system
spec:
  imageRepositoryRef:
    name: my-app
  policy:
    alphabetical:
      order: asc
  filterTags:
    pattern: "^develop-[a-f0-9]+-(?P<ts>[0-9]+)"
    extract: "$ts"
```

### ImagePolicy - Numerical

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImagePolicy
metadata:
  name: my-app-build
  namespace: flux-system
spec:
  imageRepositoryRef:
    name: my-app
  policy:
    numerical:
      order: asc
  filterTags:
    pattern: "^build-(?P<num>[0-9]+)"
    extract: "$num"
```

### ImageUpdateAutomation

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageUpdateAutomation
metadata:
  name: my-app
  namespace: flux-system
spec:
  interval: 1m0s
  sourceRef:
    kind: GitRepository
    name: flux-system
  git:
    checkout:
      ref:
        branch: main
    commit:
      author:
        email: fluxcdbot@users.noreply.github.com
        name: fluxcdbot
      messageTemplate: |
        Automated image update

        Automation name: {{ .AutomationObject }}

        Files:
        {{ range $filename, $_ := .Updated.Files -}}
        - {{ $filename }}
        {{ end -}}

        Objects:
        {{ range $resource, $_ := .Updated.Objects -}}
        - {{ $resource.Kind }} {{ $resource.Name }}
        {{ end -}}

        Images:
        {{ range .Updated.Images -}}
        - {{.}}
        {{ end -}}
  update:
    path: ./apps/production
    strategy: Setters
```

### Manifest with Image Update Markers

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  namespace: apps
spec:
  template:
    spec:
      containers:
        - name: app
          image: ghcr.io/org/my-app:1.0.0 # {"$imagepolicy": "flux-system:my-app"}
```

### Image Automation Best Practices

**Environment Strategy:**

- Enable automation in development/staging first
- Use manual approval for production (PR-based workflow)
- Test policy rules before deploying

**Tag Policies:**

- Use semver for releases (e.g., `1.0.x`, `>=1.0.0`)
- Use regex for branch-based tags (e.g., `^develop-.*`)
- Use numerical for build numbers

**Security:**

- Scan images before deployment (integrate with CI)
- Use private registries with authentication
- Enable image signing verification

### ImageUpdateAutomation with Push Branch

For PR-based workflows:

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageUpdateAutomation
metadata:
  name: my-app
  namespace: flux-system
spec:
  interval: 1m0s
  sourceRef:
    kind: GitRepository
    name: flux-system
  git:
    checkout:
      ref:
        branch: main
    push:
      branch: image-updates
    commit:
      author:
        email: fluxcdbot@users.noreply.github.com
        name: fluxcdbot
      messageTemplate: |
        Automated image update by Flux

        [ci skip]
  update:
    path: ./apps/production
    strategy: Setters
```

## Notifications

### Slack Provider

```yaml
apiVersion: notification.toolkit.fluxcd.io/v1beta3
kind: Provider
metadata:
  name: slack
  namespace: flux-system
spec:
  type: slack
  channel: flux-notifications
  secretRef:
    name: slack-webhook-url
---
apiVersion: v1
kind: Secret
metadata:
  name: slack-webhook-url
  namespace: flux-system
stringData:
  address: https://hooks.slack.com/services/YOUR/WEBHOOK/URL
```

### Alert for Kustomization Failures

```yaml
apiVersion: notification.toolkit.fluxcd.io/v1beta3
kind: Alert
metadata:
  name: kustomization-failures
  namespace: flux-system
spec:
  providerRef:
    name: slack
  eventSeverity: error
  eventSources:
    - kind: Kustomization
      name: "*"
  exclusionList:
    - ".*health check failed.*"
```

### Alert for HelmRelease Events

```yaml
apiVersion: notification.toolkit.fluxcd.io/v1beta3
kind: Alert
metadata:
  name: helm-releases
  namespace: flux-system
spec:
  providerRef:
    name: slack
  eventSeverity: info
  eventSources:
    - kind: HelmRelease
      name: "*"
      namespace: "*"
  summary: "Helm Release {{ .InvolvedObject.name }} in {{ .InvolvedObject.namespace }}"
```

### Microsoft Teams Provider

```yaml
apiVersion: notification.toolkit.fluxcd.io/v1beta3
kind: Provider
metadata:
  name: msteams
  namespace: flux-system
spec:
  type: msteams
  secretRef:
    name: msteams-webhook-url
---
apiVersion: v1
kind: Secret
metadata:
  name: msteams-webhook-url
  namespace: flux-system
stringData:
  address: https://outlook.office.com/webhook/YOUR/WEBHOOK/URL
```

### Receiver for GitHub Webhooks

```yaml
apiVersion: notification.toolkit.fluxcd.io/v1
kind: Receiver
metadata:
  name: github-receiver
  namespace: flux-system
spec:
  type: github
  events:
    - "ping"
    - "push"
  secretRef:
    name: github-webhook-token
  resources:
    - kind: GitRepository
      name: flux-system
---
apiVersion: v1
kind: Secret
metadata:
  name: github-webhook-token
  namespace: flux-system
type: Opaque
stringData:
  token: <webhook-secret>
```

## Multi-Cluster Setup

### Fleet Repository Structure

```text
fleet-infra/
├── clusters/
│   ├── production/
│   │   ├── flux-system/
│   │   └── cluster-config.yaml
│   ├── staging/
│   │   ├── flux-system/
│   │   └── cluster-config.yaml
│   └── development/
│       ├── flux-system/
│       └── cluster-config.yaml
├── infrastructure/
│   ├── base/
│   └── overlays/
│       ├── production/
│       ├── staging/
│       └── development/
└── apps/
    ├── base/
    └── overlays/
        ├── production/
        ├── staging/
        └── development/
```

### Cluster-Specific Configuration

Production cluster (`clusters/production/cluster-config.yaml`):

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: infrastructure
  namespace: flux-system
spec:
  interval: 10m0s
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./infrastructure/overlays/production
  prune: true
  wait: true
  postBuild:
    substitute:
      cluster_name: production
      cluster_region: us-east-1
      replicas: "3"
---
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: apps
  namespace: flux-system
spec:
  interval: 10m0s
  dependsOn:
    - name: infrastructure
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./apps/overlays/production
  prune: true
  postBuild:
    substitute:
      cluster_name: production
      domain: prod.example.com
```

### Multi-Cluster with Cluster API

Manage multiple clusters using Cluster API:

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: cluster-staging
  namespace: flux-system
spec:
  interval: 10m0s
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./clusters/staging
  prune: true
  kubeConfig:
    secretRef:
      name: staging-kubeconfig
---
apiVersion: v1
kind: Secret
metadata:
  name: staging-kubeconfig
  namespace: flux-system
type: Opaque
data:
  value: <base64-encoded-kubeconfig>
```

## Dependency Management

### Infrastructure Layer Dependencies

```yaml
# Base infrastructure
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: crds
  namespace: flux-system
spec:
  interval: 1h
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./infrastructure/crds
  prune: false # Never prune CRDs automatically
---
# Depends on CRDs
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: cert-manager
  namespace: flux-system
spec:
  interval: 10m
  dependsOn:
    - name: crds
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./infrastructure/cert-manager
  healthChecks:
    - apiVersion: apps/v1
      kind: Deployment
      name: cert-manager
      namespace: cert-manager
---
# Depends on cert-manager
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: ingress-nginx
  namespace: flux-system
spec:
  interval: 10m
  dependsOn:
    - name: cert-manager
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./infrastructure/ingress-nginx
```

### Application Dependencies

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: database
  namespace: flux-system
spec:
  interval: 10m
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./apps/database
  healthChecks:
    - apiVersion: apps/v1
      kind: StatefulSet
      name: postgresql
      namespace: database
---
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: backend
  namespace: flux-system
spec:
  interval: 5m
  dependsOn:
    - name: database
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./apps/backend
---
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: frontend
  namespace: flux-system
spec:
  interval: 5m
  dependsOn:
    - name: backend
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./apps/frontend
```

## Best Practices

### 1. Resource Organization

- **Separate concerns**: Keep infrastructure, apps, and cluster configs in separate directories
- **Use overlays**: Leverage Kustomize overlays for environment-specific configurations
- **Namespace isolation**: Use separate namespaces for different teams or applications

### 2. Reconciliation Intervals

- **Infrastructure**: 1h (stable resources that change infrequently)
- **Applications**: 10m (balance between responsiveness and API load)
- **Development**: 1m-5m (faster feedback during active development)
- **Source repos**: 1m-5m (detect changes quickly)

### 3. Pruning Strategy

- **Enable pruning**: Set `prune: true` for Kustomizations to clean up deleted resources
- **CRDs exception**: Set `prune: false` for CRD Kustomizations to prevent accidental deletion
- **Test before production**: Test pruning in non-production environments first

### 4. Health Checks

Always define health checks for critical resources:

```yaml
spec:
  healthChecks:
    - apiVersion: apps/v1
      kind: Deployment
      name: critical-app
      namespace: apps
    - apiVersion: v1
      kind: Service
      name: critical-service
      namespace: apps
```

### 5. Suspend Reconciliation

Temporarily suspend reconciliation when needed:

```bash
# Suspend a Kustomization
flux suspend kustomization apps

# Resume reconciliation
flux resume kustomization apps
```

### 6. Force Reconciliation

Trigger immediate reconciliation:

```bash
# Reconcile a specific Kustomization
flux reconcile kustomization apps --with-source

# Reconcile a HelmRelease
flux reconcile helmrelease my-app -n apps
```

### 7. Monitoring and Debugging

```bash
# Check Flux components status
flux check

# Get all Flux resources
flux get all

# Get specific resource with detailed info
flux get kustomization infrastructure

# View logs
flux logs --level=error --all-namespaces

# Export current cluster state
flux export source git flux-system
flux export kustomization --all
```

### 8. Version Control

- **Commit frequently**: Small, atomic commits are easier to debug
- **Meaningful messages**: Describe what and why, not just what
- **Branch protection**: Require reviews for main/production branches
- **Tag releases**: Use Git tags for application version tracking

### 9. Security

- **Encrypt secrets**: Always use SOPS or external secret managers
- **RBAC**: Implement strict RBAC policies for multi-tenancy
- **Network policies**: Define network policies for namespace isolation
- **Image scanning**: Integrate container image scanning in CI/CD
- **Policy enforcement**: Use tools like OPA Gatekeeper or Kyverno

### 10. Disaster Recovery

```bash

# Backup Flux configuration
flux export source git --all > sources.yaml
flux export kustomization --all > kustomizations.yaml
flux export helmrelease --all > helmreleases.yaml

# Restore from backup
kubectl apply -f sources.yaml
kubectl apply -f kustomizations.yaml
kubectl apply -f helmreleases.yaml
```

## Common Patterns

### Progressive Delivery with Flagger

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: flagger
  namespace: flagger-system
spec:
  interval: 10m
  chart:
    spec:
      chart: flagger
      version: "1.x"
      sourceRef:
        kind: HelmRepository
        name: flagger
        namespace: flux-system
---
apiVersion: flagger.app/v1beta1
kind: Canary
metadata:
  name: my-app
  namespace: apps
spec:
  targetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: my-app
  service:
    port: 80
  analysis:
    interval: 1m
    threshold: 5
    maxWeight: 50
    stepWeight: 10
    metrics:
      - name: request-success-rate
        thresholdRange:
          min: 99
        interval: 1m
```

### External Secrets Operator Integration

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: external-secrets
  namespace: flux-system
spec:
  interval: 10m
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./infrastructure/external-secrets
  prune: true
---
apiVersion: external-secrets.io/v1beta1
kind: SecretStore
metadata:
  name: aws-secretsmanager
  namespace: apps
spec:
  provider:
    aws:
      service: SecretsManager
      region: us-east-1
      auth:
        jwt:
          serviceAccountRef:
            name: external-secrets-sa
---
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: app-secrets
  namespace: apps
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secretsmanager
    kind: SecretStore
  target:
    name: app-secrets
    creationPolicy: Owner
  data:
    - secretKey: db-password
      remoteRef:
        key: prod/app/database
        property: password
```

## Troubleshooting

### Common Issues

**Issue**: Kustomization stuck in "Progressing" state

```bash
# Check Kustomization status
flux get kustomization infrastructure

# View detailed events
kubectl describe kustomization infrastructure -n flux-system

# Check logs
kubectl logs -n flux-system deploy/kustomize-controller
```

**Issue**: HelmRelease installation failed

```bash
# Get HelmRelease status
flux get helmrelease my-app -n apps

# View Helm release history
helm history my-app -n apps

# Check Helm controller logs
kubectl logs -n flux-system deploy/helm-controller
```

**Issue**: Image automation not updating manifests

```bash
# Check ImageRepository status
flux get image repository my-app

# Check ImagePolicy status
flux get image policy my-app

# View image automation logs
kubectl logs -n flux-system deploy/image-reflector-controller
kubectl logs -n flux-system deploy/image-automation-controller
```

**Issue**: Source reconciliation failures

```bash
# Check GitRepository status
flux get source git flux-system

# View source controller logs
kubectl logs -n flux-system deploy/source-controller

# Reconcile manually
flux reconcile source git flux-system
```

### Debug Mode

Enable debug logging:

```bash
# Patch controller for debug logging
kubectl patch deployment kustomize-controller \
  -n flux-system \
  --type='json' \
  -p='[{"op": "add", "path": "/spec/template/spec/containers/0/args/-", "value": "--log-level=debug"}]'
```

## Performance Optimization

### Reduce API Server Load

```yaml
spec:
  interval: 1h # Increase for stable resources
  retryInterval: 5m # Retry less frequently on errors
```

### Optimize Git Operations

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata:
  name: flux-system
  namespace: flux-system
spec:
  interval: 5m
  ref:
    branch: main
  url: https://github.com/org/repo
  ignore: |
    # Reduce clone size
    *.md
    docs/
    examples/
```

### Parallel Reconciliation

Controller concurrency is tuned with the `--concurrent` arg on each controller
Deployment, not via `flux install` flags (there is no `--kustomize-concurrency`
or `--helm-concurrency`). Apply it as a Kustomize patch in
`clusters/<env>/flux-system/` so the change is stored in Git and self-managed by
the bootstrap kustomization — prefer `flux bootstrap` over `flux install` for
exactly this reason (the components become version-controlled).

```yaml
# clusters/<env>/flux-system/kustomization.yaml
patches:
  - patch: |
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --concurrent=10
    target:
      kind: Deployment
      name: "(kustomize-controller|helm-controller)"
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

The patterns above get a cluster running; this section captures the
non-obvious behavior that separates a working install from a correct one. Most
of these are silent failures — no error, just wrong behavior.

### Currency: stable API versions

**Use stable APIs; beta versions are removed in Flux 2.7+.** Flux promoted its
core APIs to stable and removed the betas, so any manifest on a beta
`apiVersion` is rejected after a CRD upgrade — there is no compatibility
shim. The mapping:

- `HelmRelease` → `helm.toolkit.fluxcd.io/v2` (stable since Flux 2.3, May 2024).
- `HelmRepository` / `HelmChart` / `OCIRepository` → `source.toolkit.fluxcd.io/v1`.
- `ImageRepository` / `ImagePolicy` / `ImageUpdateAutomation` → `image.toolkit.fluxcd.io/v1` (promoted in Flux 2.7, Sep 2025, which removed the older betas).

The v2 `HelmRelease` API also dropped three fields with no in-place equivalent:
`.spec.chart.spec.valuesFile` (use `valuesFiles`, plural) and
`postRenderers.kustomize.patchesJson6902` / `patchesStrategicMerge` (both
unified into `patches`). Before upgrading the controllers, mechanically rewrite
manifests with `flux migrate -f <path> -v <target-version>`.

### Design patterns (idioms)

**Prefer `chartRef` + `OCIRepository` over `chart.spec` for shared, pinned,
signed charts.** `chart.spec` creates a hidden managed `HelmChart` per
`HelmRelease`, pinnable only by version/semver. `chartRef` points at an existing
`OCIRepository` (or `HelmChart`) so multiple releases share one source, supports
digest pinning for immutable deploys, and enables Cosign/notation verification
on the source. The two fields are mutually exclusive; `HelmRepository type: oci`
is now in maintenance mode. Switching an existing release from `chart.spec` to
`chartRef` performs a Helm **upgrade** (not uninstall+reinstall) and garbage-collects the old `HelmChart`.

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: OCIRepository
metadata:
  name: podinfo-chart
  namespace: flux-system
spec:
  interval: 12h
  url: oci://ghcr.io/stefanprodan/charts/podinfo
  ref:
    digest: sha256:a0d3...   # immutable pin
  verify:
    provider: cosign
    secretRef:
      name: cosign-pub
---
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: podinfo
  namespace: apps
spec:
  interval: 10m
  chartRef:
    kind: OCIRepository
    name: podinfo-chart
    namespace: flux-system
```

**Set `retryInterval` independently from `interval`.** They are orthogonal
timers: `interval` is the steady-state drift-detection cadence (minimum 60s),
`retryInterval` is the failure-recovery cadence and defaults to `interval` when
unset. An infrastructure resource at `interval: 1h` therefore waits a full hour
to retry a transient failure unless you lower `retryInterval`.

```yaml
spec:
  interval: 1h        # hourly drift detection — low API load
  retryInterval: 2m   # fast recovery from transient failures
  prune: true
```

**Label referenced ConfigMaps/Secrets `reconcile.fluxcd.io/watch: Enabled`.**
By default Flux only re-reconciles on the interval tick, so editing a ConfigMap
used by `postBuild.substituteFrom` or a Secret used by `valuesFrom` is not
picked up until the next scheduled reconcile (possibly hours away). The label
(added in Flux 2.7) makes the controller watch the object and reconcile
immediately on change; the HelmRelease docs recommend it for every Secret/ConfigMap in `valuesFrom`.

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: cluster-vars
  namespace: flux-system
  labels:
    reconcile.fluxcd.io/watch: Enabled
data:
  cluster_name: production
```

### Gotchas (silent failures)

**`wait: true` silently ignores `healthChecks` — they are mutually exclusive.**
When `wait` is true the Kustomization health-checks *all* reconciled resources
and `.spec.healthChecks` is ignored entirely. Setting both gives a false sense
of targeted gating while actually checking everything. To gate on a named
subset, leave `wait` unset/false and use `healthChecks` alone.

**`postBuild` substitution has several silent traps.** (a) Substitution only
runs if at least one `substitute` var or `substituteFrom` source is defined —
with both empty the feature is off and `${var:=default}` passes through
literally. (b) Variable names must match `^[_[:alpha:]][_[:alpha:][:digit:]]*$`
— a hyphen or dot means the var is silently not substituted. (c) An undefined
`${VAR}` with no default is replaced with an empty string, so a typo like
`${cluster_rgion}` silently corrupts a URL with no error. (d) Quote numbers and
booleans to avoid YAML coercion of type-sensitive fields. Fix (c) with the
controller flag `--feature-gates=StrictPostBuildSubstitutions=true` (fail-fasts
on undefined vars) and validate locally with
`flux build kustomization --strict-substitute`.

```yaml
spec:
  postBuild:
    substitute:
      cluster_name: production
      replicas: "3"        # quoted — avoids YAML int coercion
      tls_enabled: "true"  # quoted — avoids YAML bool coercion
    substituteFrom:
      - kind: ConfigMap
        name: cluster-vars
        optional: true
```

**Renaming a Kustomization (or moving resources between two) with `prune: true`
deletes its managed workloads.** Flux tracks owned resources in
`.status.inventory` keyed by the object's name+namespace. Rename the object and
its entire inventory is garbage-collected, then recreated — a momentary outage.
Safe procedure: set `prune: false`, reconcile, verify the renamed object is
Ready and owns the resources, then re-enable pruning. The per-resource
annotation `kustomize.toolkit.fluxcd.io/prune: disabled` opts a resource out of
GC during the transition.

```bash
kubectl patch kustomization my-app -n flux-system --type=merge -p '{"spec":{"prune":false}}'
flux reconcile kustomization my-app
# rename in Git, push, let Flux adopt under the new name, THEN re-enable prune
```

**HelmRelease drift detection is `Disabled` by default.** The helm-controller
does NOT detect or correct out-of-band changes unless you set
`spec.driftDetection.mode`; `kubectl` edits diverge silently until the next Helm
action. `mode: warn` logs drift via events without correcting; `mode: enabled`
corrects via server-side dry-run apply. Companion trap: once enabled, any
controller that legitimately mutates Helm-managed resources (HPA on
`/spec/replicas`, VPA on resources, cert-manager CA injection) gets reverted
every cycle — add `driftDetection.ignore` JSON-pointer paths for those. Start
with `mode: warn` to discover them.

```yaml
spec:
  driftDetection:
    mode: enabled
    ignore:
      - paths: ["/spec/replicas"]   # managed by HPA
        target:
          kind: Deployment
```

**HelmRelease `valuesFrom` with `targetPath` has the HIGHEST precedence — above
inline `spec.values`.** `valuesFrom` entries merge left-to-right, then inline
`values` overwrites those — BUT a `valuesFrom` entry that sets `targetPath`
overwrites everything before it, including inline values. Teams assuming inline
values always win get silently overridden. (Separately: deleting a
ConfigMap/Secret referenced in `valuesFrom` changes the release inputs and
triggers a Helm upgrade, not just a reload.)

**HelmRelease `upgrade.remediation` defaults are asymmetric.**
`install.remediation.remediateLastFailure` defaults to `false`.
`upgrade.remediation.remediateLastFailure` also defaults to `false` UNLESS
`.retries > 0`, in which case it defaults to `true` — so merely adding an
upgrade retry count silently enables last-failure rollback to the previous
release, which can surprise operators. Be explicit, pair with `cleanupOnFail`,
and avoid `retries: -1` (unlimited) on a broken chart.

```yaml
spec:
  install:
    remediation:
      retries: 3
      remediateLastFailure: true   # explicit
  upgrade:
    remediation:
      retries: 3
      remediateLastFailure: true   # explicit — not relying on the implicit default
    cleanupOnFail: true
```

**HelmRelease release name is silently SHA-256-truncated past 53 chars.** Flux
composes the Helm release name as `[<targetNamespace>-]<HelmRelease.name>`. When
that exceeds Helm's 53-character DNS-label limit, Flux shortens it to the first
40 characters of the name, a dash, then the first 12 characters of a SHA-256
hash of the full composed name. `helm list`/`helm history` then will not show
the expected name. Set `spec.releaseName` explicitly whenever the composed name
could approach 53 chars (especially with long namespaces).

**`kubectl rollout restart` on a Flux-managed resource churns.** It adds a
`kubectl.kubernetes.io/restartedAt` annotation; the next reconcile removes it
(it is not in Git) and redeploys the pods — a loop during active reconciliation.
Use the `flux-client-side-apply` field manager so Flux respects the annotation.
(Likewise any `kubectl edit` to a managed resource is reverted next interval —
intentional drift correction.)

```bash
kubectl rollout restart deployment/my-app -n apps --field-manager=flux-client-side-apply
```

**`filterTags.extract` drops non-matching tags entirely — there is no
fallback.** In `ImagePolicy.filterTags`, `pattern` selects which tags are
considered and `extract` supplies a derived value (e.g. a captured timestamp) to
the sorting policy in place of the tag — it does not rename, alias, or fall
back. Tags failing the pattern are dropped from consideration; a wrong regex
yields zero candidates and "no latest image" rather than reverting to all tags.
Companion rule: `digestReflectionPolicy: Always` requires an `interval` field,
while `IfNotPresent`/`Never` forbid it.

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImagePolicy
spec:
  policy:
    alphabetical:
      order: asc
  filterTags:
    pattern: "^main-[a-f0-9]+-(?P<ts>[0-9]{10})$"  # named capture group
    extract: "$ts"   # only the timestamp is passed to the comparator
```

**Image automation needs a read-write deploy key; re-bootstrapping does NOT
rotate it.** `flux bootstrap` creates a read-only deploy key by default, so
image-automation-controller (which must push commits) silently fails without
`--read-write-key`. Re-running bootstrap with `--read-write-key` after an
earlier read-only bootstrap does NOT overwrite the existing `flux-system`
Secret — delete that Secret first, then re-bootstrap, to actually rotate to a
write-capable key. Also, `ImageUpdateAutomation` evaluates only `ImagePolicy`
objects in its own namespace — cross-namespace policy references are not
supported.

```bash
flux bootstrap github \
  --components-extra=image-reflector-controller,image-automation-controller \
  --read-write-key \
  --owner=$GITHUB_USER --repository=fleet-infra --branch=main --path=clusters/production

# To rotate an existing read-only bootstrap:
kubectl delete secret flux-system -n flux-system
flux bootstrap github --read-write-key ...   # secret recreated with write key
```

**The two `Kustomization` kinds are different objects.**
`kustomization.kustomize.toolkit.fluxcd.io` is the Flux custom resource (a
reconciliation unit sourcing from a GitRepository/OCIRepository and optionally
applying an overlay); `kustomization.kustomize.config.k8s.io` is the native
kustomize file format. The Flux CR's `spec.path` points at a directory
containing a `kustomization.yaml` of the config kind — it orchestrates, it does
not replace it. Native fields (`resources`, `patches`, `configMapGenerator`)
belong in the file, never in the Flux CR `spec`.

### Anti-patterns

**Never bind a tenant reconciler to `cluster-admin`** (or any
`ClusterRoleBinding`) — it defeats namespace isolation. Use a namespace-scoped
`RoleBinding` to a custom `Role` or to the built-in `admin` ClusterRole, and
combine with the lockdown flags above. (Fixed in the Multi-Tenancy section.)

**`force: true` is a temporary escape hatch, not a setting.** It makes the
controller replace resources in-cluster when patching fails on an immutable-field
change (delete-then-recreate), bypassing Kubernetes immutability guards for
EVERY managed resource. Left on permanently it removes the protection against
accidental data loss on stateful workloads. Prefer the per-resource annotation
`kustomize.toolkit.fluxcd.io/force: enabled` on the one object that needs it,
then remove it.

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: db-migration
  annotations:
    kustomize.toolkit.fluxcd.io/force: enabled   # remove after the spec change merges
```

### Security

**Multi-tenancy is not enforced by default.** See the
[Multi-Tenancy Lockdown Flags](#multi-tenancy-lockdown-flags-mandatory)
subsection — `--no-cross-namespace-refs`, `--no-remote-bases`, and
`--default-service-account` are mandatory; omitting any one leaves a
privilege-escalation path that RBAC alone does not close.

**Ban Kustomize remote bases in production.** Bases pointing at external URLs
(e.g. `github.com/org/repo/path?ref=main`) are fetched at reconcile time over
HTTPS, outside Flux's GitRepository/OCIRepository artifact pipeline: no
cryptographic verification, no caching (refetched every cycle), no immutability,
and absent from source history. In a multi-tenant cluster this is a supply-chain
risk. Disable with `--no-remote-bases=true` and replace remote bases with a Flux
`OCIRepository`/`GitRepository` source pinned by digest.

**Use workload identity instead of static credential Secrets.** Flux 2.7
completed object-level Kubernetes Workload Identity for all Flux APIs that
authenticate to cloud providers (GitRepository, OCIRepository, ImageRepository,
Bucket, Kustomization, HelmRelease, Provider) on AWS (EKS IRSA), Azure (AKS
Workload Identity), and GCP (GKE Workload Identity). Set `.spec.provider:
aws|azure|gcp` so the controller fetches short-lived OIDC tokens instead of
reading a static Secret — no rotation burden, smaller blast radius on leak.

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageRepository
metadata:
  name: my-app
  namespace: flux-system
spec:
  image: 012345678901.dkr.ecr.us-east-1.amazonaws.com/my-app
  interval: 5m
  provider: aws   # IRSA — no secretRef
```

## Summary

Flux CD provides a powerful, declarative approach to managing Kubernetes deployments through GitOps. Key takeaways:

1. **Bootstrap once**: Use `flux bootstrap` to set up Flux in your cluster
2. **Organize thoughtfully**: Structure your repository for clarity and maintainability
3. **Layer dependencies**: Build infrastructure before applications
4. **Secure secrets**: Use SOPS or external secret managers
5. **Monitor actively**: Set up alerts and regularly check Flux status
6. **Automate carefully**: Use image automation for non-production environments first
7. **Multi-tenancy**: Leverage namespaces and RBAC for team isolation
8. **Test changes**: Validate in lower environments before production

### Key Decision Points

**Choose GitRepository vs HelmRepository:**

- GitRepository: For custom manifests, Kustomize overlays, or Helm charts in Git
- HelmRepository: For public/private Helm chart repositories

**Choose Kustomization vs HelmRelease:**

- Kustomization: For raw manifests, ConfigMaps, Secrets, Kustomize overlays
- HelmRelease: For packaged Helm charts with values customization

**Image Automation Strategy:**

- Direct commit: Development/staging environments with rapid iteration
- PR workflow: Production environments requiring review and approval
- Disabled: Mission-critical production with manual deployment gates

**Multi-Tenancy Approach:**

- Namespace isolation: Teams share cluster, separate by namespace
- Cluster isolation: Each team gets dedicated cluster(s)
- Hybrid: Core teams share, external teams isolated

**Secret Management:**

- SOPS: Git-native, age/pgp encryption, good for small teams
- External Secrets Operator: Integrate AWS Secrets Manager, Vault, GCP Secret Manager
- Sealed Secrets: Kubernetes-native, one-way encryption

By following these patterns and practices, you can build reliable, automated deployment pipelines that scale with your organization.
