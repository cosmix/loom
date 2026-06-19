---
name: loom-argocd
description: GitOps continuous delivery with Argo CD for Kubernetes. Use when implementing declarative GitOps workflows, application sync/rollback, multi-cluster deployments, progressive delivery, or CD automation.
allowed-tools:
  - Read
  - Edit
  - Write
  - Bash
triggers:
  - argocd
  - argo cd
  - gitops
  - application
  - sync
  - rollback
  - app of apps
  - applicationset
  - declarative
  - continuous delivery
  - CD
  - deployment automation
  - kubernetes deployment
  - multi-cluster
  - canary deployment
  - blue-green
---

# Argo CD GitOps Continuous Delivery

## Overview

Argo CD is a declarative, GitOps continuous delivery tool for Kubernetes that automates application deployment and lifecycle management. It follows the GitOps pattern where Git repositories are the source of truth for defining the desired application state.

### Core Concepts

- **Application**: A group of Kubernetes resources defined by a manifest in Git
- **Application Source Type**: The tool/format used to define the application (Helm, Kustomize, plain YAML, Jsonnet)
- **Target State**: The desired state of an application as represented in Git
- **Live State**: The actual state of an application running in Kubernetes
- **Sync Status**: Whether the live state matches the target state
- **Sync**: The process of making the live state match the target state
- **Health**: The health status of application resources
- **Refresh**: Compare the latest code in Git with the live state
- **Project**: A logical grouping of applications with RBAC policies

## Installation and Setup

### Install Argo CD in Kubernetes

```bash
# Create namespace
kubectl create namespace argocd

# Install Argo CD
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml

# Install with HA (production)
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/ha/install.yaml

# Access the UI
kubectl port-forward svc/argocd-server -n argocd 8080:443

# Get initial admin password
kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath="{.data.password}" | base64 -d

# Install CLI
brew install argocd  # macOS
# Or download from https://github.com/argoproj/argo-cd/releases
```

### Initial Configuration

```bash
# Login via CLI
argocd login localhost:8080

# Change admin password
argocd account update-password

# Register external cluster
argocd cluster add my-cluster-context

# Add Git repository
argocd repo add https://github.com/myorg/myrepo.git --username myuser --password mytoken
```

## Repository Structure

### Recommended Directory Layout

```text
gitops-repo/
├── apps/                           # Application definitions
│   ├── base/                       # Base application configs
│   │   ├── app1/
│   │   │   ├── kustomization.yaml
│   │   │   └── deployment.yaml
│   │   └── app2/
│   └── overlays/                   # Environment-specific overlays
│       ├── dev/
│       │   ├── kustomization.yaml
│       │   └── patches/
│       ├── staging/
│       └── production/
├── charts/                         # Helm charts (if using Helm)
│   └── myapp/
│       ├── Chart.yaml
│       ├── values.yaml
│       └── templates/
├── argocd/                         # Argo CD configuration
│   ├── projects/                   # AppProjects
│   ├── applications/               # Application manifests
│   │   ├── app1.yaml
│   │   └── app2.yaml
│   └── applicationsets/            # ApplicationSets
│       ├── cluster-apps.yaml
│       └── tenant-apps.yaml
└── bootstrap/                      # App of apps bootstrap
    └── root-app.yaml
```

### Separation Strategies

#### Mono-repo

Single repository for all environments

- Pros: Simpler management, easier to track changes
- Cons: All teams have access, harder to enforce separation

#### Repo-per-environment

Separate repositories for dev/staging/prod

- Pros: Better security boundaries, clear promotion path
- Cons: More repositories to manage, duplicate configuration

#### Repo-per-team

Separate repositories per team/service

- Pros: Team autonomy, clear ownership
- Cons: Cross-team coordination complexity

## Application Manifests

### Basic Application

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp
  namespace: argocd
  # Finalizer ensures cascade delete
  finalizers:
    - resources-finalizer.argocd.argoproj.io
spec:
  # Project name (default is 'default')
  project: default

  # Source configuration
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/production/myapp

  # Destination cluster and namespace
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp-production

  # Sync policy
  syncPolicy:
    automated:
      prune: true # Delete resources not in Git
      selfHeal: true # Auto-sync when cluster state differs
      allowEmpty: false
    syncOptions:
      - CreateNamespace=true
      - PrunePropagationPolicy=foreground
      - PruneLast=true
    retry:
      limit: 5
      backoff:
        duration: 5s
        factor: 2
        maxDuration: 3m
```

### Application with Helm

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-helm
  namespace: argocd
spec:
  project: default

  source:
    repoURL: https://github.com/myorg/charts.git
    targetRevision: main
    path: charts/myapp
    helm:
      # Helm values files
      valueFiles:
        - values.yaml
        - values-production.yaml

      # Inline values (highest priority)
      values: |
        replicaCount: 3
        image:
          tag: v1.2.3
        resources:
          limits:
            cpu: 500m
            memory: 512Mi

      # Override specific values
      parameters:
        - name: image.repository
          value: myregistry.io/myapp

      # Skip CRDs installation
      skipCrds: false

      # Release name
      releaseName: myapp

  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
      - CreateNamespace=true
```

### Application with Kustomize

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-kustomize
  namespace: argocd
spec:
  project: default

  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/overlays/production
    kustomize:
      # Kustomize version
      version: v5.0.0

      # Name prefix/suffix
      namePrefix: prod-
      nameSuffix: -v1

      # Images to override
      images:
        - name: myapp
          newName: myregistry.io/myapp
          newTag: v1.2.3

      # Common labels
      commonLabels:
        environment: production
        managed-by: argocd

      # Common annotations
      commonAnnotations:
        deployed-by: argocd

      # Replicas override
      replicas:
        - name: myapp-deployment
          count: 3

  destination:
    server: https://kubernetes.default.svc
    namespace: myapp-production
```

## ApplicationSets

### Cluster Generator

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: cluster-apps
  namespace: argocd
spec:
  # Generate applications for all registered clusters
  generators:
    - clusters:
        selector:
          matchLabels:
            env: production
          matchExpressions:
            - key: region
              operator: In
              values: [us-east-1, us-west-2]
        values:
          # Default values available in template
          revision: main

  template:
    metadata:
      name: "{{name}}-myapp"
      labels:
        cluster: "{{name}}"
    spec:
      project: default
      source:
        repoURL: https://github.com/myorg/myrepo.git
        targetRevision: "{{values.revision}}"
        path: apps/production/myapp
        helm:
          parameters:
            - name: cluster.name
              value: "{{name}}"
            - name: cluster.region
              value: "{{metadata.labels.region}}"
      destination:
        server: "{{server}}"
        namespace: myapp
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
        syncOptions:
          - CreateNamespace=true
```

### Git Directory Generator

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: git-directory-apps
  namespace: argocd
spec:
  generators:
    - git:
        repoURL: https://github.com/myorg/myrepo.git
        revision: HEAD
        directories:
          - path: apps/production/*
          - path: apps/production/exclude-this
            exclude: true

  template:
    metadata:
      name: "{{path.basename}}"
    spec:
      project: default
      source:
        repoURL: https://github.com/myorg/myrepo.git
        targetRevision: HEAD
        path: "{{path}}"
      destination:
        server: https://kubernetes.default.svc
        namespace: "{{path.basename}}"
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
```

### Git File Generator

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: git-file-apps
  namespace: argocd
spec:
  generators:
    - git:
        repoURL: https://github.com/myorg/myrepo.git
        revision: HEAD
        files:
          - path: apps/*/config.json

  template:
    metadata:
      name: "{{app.name}}"
    spec:
      project: default
      source:
        repoURL: https://github.com/myorg/myrepo.git
        targetRevision: HEAD
        path: "apps/{{app.name}}"
        helm:
          parameters:
            - name: replicaCount
              value: "{{app.replicas}}"
            - name: environment
              value: "{{app.environment}}"
      destination:
        server: https://kubernetes.default.svc
        namespace: "{{app.namespace}}"
```

### Matrix Generator

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: matrix-apps
  namespace: argocd
spec:
  generators:
    # Matrix combines multiple generators
    - matrix:
        generators:
          # First dimension: clusters
          - clusters:
              selector:
                matchLabels:
                  env: production
          # Second dimension: git directories
          - git:
              repoURL: https://github.com/myorg/myrepo.git
              revision: HEAD
              directories:
                - path: apps/*

  template:
    metadata:
      name: "{{path.basename}}-{{name}}"
    spec:
      project: default
      source:
        repoURL: https://github.com/myorg/myrepo.git
        targetRevision: HEAD
        path: "{{path}}"
      destination:
        server: "{{server}}"
        namespace: "{{path.basename}}"
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
```

### List Generator (Multi-tenancy)

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: tenant-apps
  namespace: argocd
spec:
  generators:
    - list:
        elements:
          - tenant: team-a
            namespace: team-a-prod
            repoURL: https://github.com/team-a/apps.git
            quota:
              cpu: "10"
              memory: 20Gi
          - tenant: team-b
            namespace: team-b-prod
            repoURL: https://github.com/team-b/apps.git
            quota:
              cpu: "20"
              memory: 40Gi

  template:
    metadata:
      name: "{{tenant}}-app"
      labels:
        tenant: "{{tenant}}"
    spec:
      project: "{{tenant}}"
      source:
        repoURL: "{{repoURL}}"
        targetRevision: main
        path: production
      destination:
        server: https://kubernetes.default.svc
        namespace: "{{namespace}}"
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
```

## ApplicationSet Patterns

### When to Use ApplicationSets

ApplicationSets automate creation and management of multiple Argo CD applications using generators. Use when:

- Deploying to multiple clusters with same configuration
- Managing multiple tenants or teams
- Discovering applications from Git repository structure
- Implementing environment promotion strategies

### Generator Selection Guide

| Generator         | Use Case                                    | Example                               |
| ----------------- | ------------------------------------------- | ------------------------------------- |
| **Cluster**       | Deploy same app to multiple clusters        | Multi-region deployment               |
| **Git Directory** | Generate apps from repo directory structure | Monorepo with app-per-directory       |
| **Git File**      | Generate apps from config files in Git      | JSON/YAML config per app              |
| **List**          | Static list of parameters                   | Tenant definitions                    |
| **Matrix**        | Combine multiple generators                 | Apps across clusters and environments |
| **Pull Request**  | Preview environments per PR                 | Ephemeral test environments           |
| **SCM Provider**  | Discover repos from GitHub/GitLab           | Org-wide app discovery                |

### Multi-Environment with Git Directory

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: multi-env-apps
  namespace: argocd
spec:
  generators:
    - matrix:
        generators:
          # First: discover apps from directory structure
          - git:
              repoURL: https://github.com/myorg/apps.git
              revision: HEAD
              directories:
                - path: apps/*
          # Second: apply to multiple environments
          - list:
              elements:
                - env: dev
                  cluster: https://dev-cluster.example.com
                  replicas: "1"
                - env: staging
                  cluster: https://staging-cluster.example.com
                  replicas: "2"
                - env: production
                  cluster: https://prod-cluster.example.com
                  replicas: "3"

  template:
    metadata:
      name: "{{path.basename}}-{{env}}"
      labels:
        app: "{{path.basename}}"
        env: "{{env}}"
    spec:
      project: default
      source:
        repoURL: https://github.com/myorg/apps.git
        targetRevision: HEAD
        path: "{{path}}"
        helm:
          parameters:
            - name: environment
              value: "{{env}}"
            - name: replicaCount
              value: "{{replicas}}"
      destination:
        server: "{{cluster}}"
        namespace: "{{path.basename}}-{{env}}"
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
        syncOptions:
          - CreateNamespace=true
```

### Pull Request Preview Environments

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: pr-preview
  namespace: argocd
spec:
  generators:
    - pullRequest:
        github:
          owner: myorg
          repo: myapp
          tokenRef:
            secretName: github-token
            key: token
          labels:
            - preview
        requeueAfterSeconds: 60

  template:
    metadata:
      name: "myapp-pr-{{number}}"
      labels:
        preview: "true"
        pr: "{{number}}"
    spec:
      project: default
      source:
        repoURL: https://github.com/myorg/myapp.git
        targetRevision: "{{head_sha}}"
        path: k8s/overlays/preview
        kustomize:
          commonLabels:
            pr: "{{number}}"
          images:
            - name: myapp
              newTag: "pr-{{number}}"
      destination:
        server: https://kubernetes.default.svc
        namespace: "myapp-pr-{{number}}"
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
        syncOptions:
          - CreateNamespace=true
```

### SCM Provider Discovery

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  name: org-repos
  namespace: argocd
spec:
  generators:
    - scmProvider:
        github:
          organization: myorg
          tokenRef:
            secretName: github-token
            key: token
        filters:
          - repositoryMatch: ".*-service$"
          - pathsExist: [k8s/production]

  template:
    metadata:
      name: "{{repository}}"
    spec:
      project: default
      source:
        repoURL: "{{url}}"
        targetRevision: main
        path: k8s/production
      destination:
        server: https://kubernetes.default.svc
        namespace: "{{repository}}"
      syncPolicy:
        automated:
          prune: true
          selfHeal: true
```

## Sync Strategies

### Strategy Selection Guide

| Strategy                    | Use Case                         | Risk   | Automation  |
| --------------------------- | -------------------------------- | ------ | ----------- |
| **Automated + SelfHeal**    | Non-prod environments            | Low    | Full        |
| **Automated (no SelfHeal)** | Staging with manual intervention | Medium | Partial     |
| **Manual**                  | Production deployments           | High   | None        |
| **Sync Windows**            | Business hours restrictions      | Medium | Scheduled   |
| **Progressive (Rollouts)**  | Gradual production rollout       | Low    | Conditional |

### Automated Sync with Conditions

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: conditional-sync
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  syncPolicy:
    automated:
      prune: true
      selfHeal: true
      allowEmpty: false

    syncOptions:
      - CreateNamespace=true
      - PruneLast=true
      - PrunePropagationPolicy=foreground
      - RespectIgnoreDifferences=true
      - ApplyOutOfSyncOnly=true

    # Retry with exponential backoff
    retry:
      limit: 5
      backoff:
        duration: 5s
        factor: 2
        maxDuration: 3m

  # Ignore manual changes to specific fields
  ignoreDifferences:
    - group: apps
      kind: Deployment
      jsonPointers:
        - /spec/replicas
    - group: ""
      kind: Service
      jqPathExpressions:
        - .spec.ports[] | select(.nodePort != null) | .nodePort
```

### Sync Windows (Time-Based Deployment Control)

```yaml
apiVersion: argoproj.io/v1alpha1
kind: AppProject
metadata:
  name: production
  namespace: argocd
spec:
  description: Production project with sync windows

  sourceRepos:
    - "*"

  destinations:
    - namespace: "*"
      server: https://prod-cluster.example.com

  # Define sync windows
  syncWindows:
    # Allow syncs during business hours (Monday-Friday 9am-5pm UTC)
    - kind: allow
      schedule: "0 9 * * 1-5"
      duration: 8h
      applications:
        - "*"
      namespaces:
        - production-*
      clusters:
        - https://prod-cluster.example.com

    # Block syncs during peak traffic (daily 12pm-2pm UTC)
    - kind: deny
      schedule: "0 12 * * *"
      duration: 2h
      applications:
        - "*"

    # Emergency sync window (manual override required)
    - kind: allow
      schedule: "* * * * *"
      duration: 1h
      manualSync: true
      applications:
        - critical-app
```

### Selective Sync (Resource-Level Control)

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: selective-sync
  namespace: argocd
  annotations:
    # Sync only specific resource types
    argocd.argoproj.io/sync-options: Prune=false
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  # Ignore specific resources from sync
  ignoreDifferences:
    - group: "*"
      kind: Secret
      name: external-secret
      jsonPointers:
        - /data

  syncPolicy:
    syncOptions:
      - CreateNamespace=true
      # Prune only specific resource types
      - PruneResourcesOnDeletion=true
```

### Blue-Green Sync Strategy

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-blue-green
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  syncPolicy:
    # Manual sync for production
    syncOptions:
      - CreateNamespace=true
```

> **Important:** there is **no `syncWaves` field** in the Argo CD `Application` CRD (neither under `syncPolicy` nor at the spec top level). Argo CD tolerates unknown fields, so such a block is silently ignored. Sync-wave ordering is configured **only** via the `argocd.argoproj.io/sync-wave` annotation on individual resource manifests — see "Sync Waves for Ordering" below. For a true blue-green strategy with traffic switching and automated promotion/abort, use **Argo Rollouts** (a separate `Rollout` CRD), not the `Application` spec.

## Rollback Procedures

### Automatic Rollback Strategies

#### Application-Level Rollback

```bash
# View sync history
argocd app history myapp

# Rollback to previous sync
argocd app rollback myapp

# Rollback to specific revision
argocd app rollback myapp 5

# Rollback with prune
argocd app rollback myapp 5 --prune
```

#### Git-Based Rollback (Recommended)

```bash
# Revert Git commit
git revert HEAD
git push origin main

# Argo CD automatically syncs the revert
# This maintains full audit trail in Git
```

### Rollback with Argo Rollouts

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: myapp
  namespace: myapp
spec:
  replicas: 5
  strategy:
    blueGreen:
      activeService: myapp-active
      previewService: myapp-preview
      autoPromotionEnabled: false
      autoPromotionSeconds: 30
      scaleDownDelaySeconds: 300
      scaleDownDelayRevisionLimit: 1

      # Automatic rollback on metric failure
      antiAffinity:
        requiredDuringSchedulingIgnoredDuringExecution: {}

  revisionHistoryLimit: 5

  selector:
    matchLabels:
      app: myapp

  template:
    metadata:
      labels:
        app: myapp
    spec:
      containers:
        - name: myapp
          image: myapp:stable
---
# Manual rollback commands
# kubectl argo rollouts abort myapp
# kubectl argo rollouts undo myapp
# kubectl argo rollouts retry myapp
```

### Rollback on Health Check Failure

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: auto-rollback-app
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  syncPolicy:
    automated:
      prune: true
      selfHeal: false # Disable selfHeal for manual rollback control

    # Retry sync on failure
    retry:
      limit: 3
      backoff:
        duration: 10s
        factor: 2
        maxDuration: 1m

  # Custom health check that triggers rollback
  syncOptions:
    - Validate=true
    - FailOnSharedResource=false


# Use PreSync hook to backup current state
---
apiVersion: batch/v1
kind: Job
metadata:
  name: pre-sync-backup
  namespace: myapp
  annotations:
    argocd.argoproj.io/hook: PreSync
    argocd.argoproj.io/hook-delete-policy: BeforeHookCreation
spec:
  template:
    spec:
      containers:
        - name: backup
          image: kubectl:latest
          command:
            - /bin/sh
            - -c
            - |
              kubectl get all -n myapp -o yaml > /backup/previous-state.yaml
      restartPolicy: Never
---
# Use SyncFail hook for automatic rollback
apiVersion: batch/v1
kind: Job
metadata:
  name: rollback-on-fail
  namespace: myapp
  annotations:
    argocd.argoproj.io/hook: SyncFail
    argocd.argoproj.io/hook-delete-policy: BeforeHookCreation
spec:
  template:
    spec:
      serviceAccountName: argocd-rollback
      containers:
        - name: rollback
          image: argoproj/argocd:latest
          command:
            - /bin/sh
            - -c
            - |
              argocd app rollback myapp --auth-token $ARGOCD_TOKEN
      restartPolicy: Never
```

### Emergency Rollback Runbook

```bash
# 1. Check application status
argocd app get myapp
argocd app history myapp

# 2. Identify last known good revision
argocd app history myapp | grep Succeeded

# 3. Disable automated sync FIRST — rollback cannot run on an app with
#    automated sync enabled (the CLI errors out otherwise)
argocd app set myapp --sync-policy none

# 4. Quick rollback to previous revision
argocd app rollback myapp

# 4b. Re-enable automated sync once the rollback is confirmed healthy
#     (only after the target revision is also reflected in Git):
#     argocd app set myapp --sync-policy automated --self-heal

# 5. If rollback fails, force sync with replace
argocd app sync myapp --force --replace --prune

# 6. If still failing, revert Git and force sync
cd gitops-repo
git revert HEAD --no-commit
git commit -m "Emergency rollback"
git push origin main
argocd app sync myapp --force

# 7. Manual resource cleanup if needed
kubectl delete deployment myapp -n myapp
argocd app sync myapp --force

# 8. Verify health and sync status
argocd app wait myapp --health --timeout 300

# 9. Document incident
echo "Rollback completed at $(date)" >> /var/log/incidents/myapp-rollback.log
```

### Rollback Testing (Pre-Production)

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: rollback-test
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp-test

  syncPolicy:
    automated:
      prune: true
      selfHeal: true


# PostSync hook to test rollback capability
---
apiVersion: batch/v1
kind: Job
metadata:
  name: test-rollback
  namespace: myapp-test
  annotations:
    argocd.argoproj.io/hook: PostSync
    argocd.argoproj.io/hook-delete-policy: BeforeHookCreation
spec:
  template:
    spec:
      serviceAccountName: argocd-test
      containers:
        - name: test
          image: argoproj/argocd:latest
          command:
            - /bin/sh
            - -c
            - |
              # Test application health
              argocd app wait rollback-test --health --timeout 60

              # Rollback CANNOT run while automated sync is enabled — disable it
              # first (the app above declares automated+selfHeal), or the CLI errors.
              argocd app set rollback-test --sync-policy none

              # Perform rollback test
              argocd app rollback rollback-test

              # Verify rollback succeeded
              argocd app wait rollback-test --health --timeout 60

              # Re-enable automated sync and re-sync to latest
              argocd app set rollback-test --sync-policy automated --self-heal
              argocd app sync rollback-test
      restartPolicy: Never
```

## App of Apps Pattern

### Root Application (Bootstrap)

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: root-app
  namespace: argocd
  # NOTE: the resources-finalizer is intentionally OMITTED on a production
  # app-of-apps root. With it present, a single `kubectl delete application
  # root-app` cascades through every child Application and wipes the whole
  # environment. Add the finalizer only on deliberately ephemeral roots
  # (e.g. PR-preview environments). See "Cascading deletion" under Expert Practices.
spec:
  project: default

  source:
    repoURL: https://github.com/myorg/gitops.git
    targetRevision: HEAD
    path: argocd/applications
    directory:
      recurse: true

  destination:
    server: https://kubernetes.default.svc
    namespace: argocd

  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
      - CreateNamespace=true
```

### Infrastructure Apps

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: infrastructure
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/gitops.git
    targetRevision: HEAD
    path: argocd/infrastructure
    directory:
      recurse: true
  destination:
    server: https://kubernetes.default.svc
    namespace: argocd
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
      - CreateNamespace=true
```

### Layered App of Apps

```text
root-app
├── infrastructure (sync-wave: 0)
│   ├── cert-manager
│   ├── ingress-nginx
│   └── external-dns
├── platform (sync-wave: 1)
│   ├── monitoring
│   ├── logging
│   └── security
└── applications (sync-wave: 2)
    ├── app1
    ├── app2
    └── app3
```

> **Prerequisite — restore the Application health check, or this ordering is a lie.** The built-in health assessment for the `argoproj.io/Application` CRD was **removed in Argo CD 1.8**. Without it, the parent treats a child Application as "done" the instant the Application *object* is applied — not when its workloads are actually Healthy — so each wave advances before the previous layer's pods (and any CRDs they install) are Ready. Restore it in `argocd-cm` (see "Restore the Application CRD health check" under Expert Practices). Even then, sync-wave annotations on **child Application objects** are reliable only at **initial bootstrap**; for ongoing ordered multi-app rollouts use **ApplicationSet Progressive Syncs (RollingSync)**, which sequences sibling Applications by design.

## Sync Waves and Hooks

### Sync Waves for Ordering

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: myapp
  annotations:
    # Lower numbers sync first
    argocd.argoproj.io/sync-wave: "0"
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: myapp-config
  namespace: myapp
  annotations:
    argocd.argoproj.io/sync-wave: "1"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
  namespace: myapp
  annotations:
    argocd.argoproj.io/sync-wave: "2"
---
apiVersion: v1
kind: Service
metadata:
  name: myapp
  namespace: myapp
  annotations:
    argocd.argoproj.io/sync-wave: "3"
```

### Resource Hooks

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: db-migration
  namespace: myapp
  annotations:
    # Hook types: PreSync, Sync, PostSync, SyncFail, Skip
    argocd.argoproj.io/hook: PreSync

    # Hook deletion policy
    argocd.argoproj.io/hook-delete-policy: HookSucceeded
    # Options: HookSucceeded, HookFailed, BeforeHookCreation

    # Sync wave for hooks
    argocd.argoproj.io/sync-wave: "1"
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: myapp:migrations
          command: ["./migrate.sh"]
      restartPolicy: Never
  backoffLimit: 3
---
apiVersion: batch/v1
kind: Job
metadata:
  name: smoke-test
  namespace: myapp
  annotations:
    argocd.argoproj.io/hook: PostSync
    argocd.argoproj.io/hook-delete-policy: HookSucceeded
spec:
  template:
    spec:
      containers:
        - name: test
          image: myapp:tests
          command: ["./smoke-test.sh"]
      restartPolicy: Never
```

## Health Checks and Resource Customizations

### Custom Health Checks

```yaml
# ConfigMap in argocd namespace
apiVersion: v1
kind: ConfigMap
metadata:
  name: argocd-cm
  namespace: argocd
data:
  # Custom health check for CRDs
  resource.customizations.health.argoproj.io_Application: |
    hs = {}
    hs.status = "Progressing"
    hs.message = ""
    if obj.status ~= nil then
      if obj.status.health ~= nil then
        hs.status = obj.status.health.status
        if obj.status.health.message ~= nil then
          hs.message = obj.status.health.message
        end
      end
    end
    return hs

  # Custom health check for Certificates
  resource.customizations.health.cert-manager.io_Certificate: |
    hs = {}
    if obj.status ~= nil then
      if obj.status.conditions ~= nil then
        for i, condition in ipairs(obj.status.conditions) do
          if condition.type == "Ready" and condition.status == "False" then
            hs.status = "Degraded"
            hs.message = condition.message
            return hs
          end
          if condition.type == "Ready" and condition.status == "True" then
            hs.status = "Healthy"
            hs.message = condition.message
            return hs
          end
        end
      end
    end
    hs.status = "Progressing"
    hs.message = "Waiting for certificate"
    return hs
```

### Resource Ignoring

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: argocd-cm
  namespace: argocd
data:
  # Ignore differences in specific fields
  resource.customizations.ignoreDifferences.apps_Deployment: |
    jsonPointers:
      - /spec/replicas
    jqPathExpressions:
      - .spec.template.spec.containers[].env[] | select(.name == "DYNAMIC_VAR")

  # Ignore differences for all resources
  resource.customizations.ignoreDifferences.all: |
    managedFieldsManagers:
      - kube-controller-manager
```

### Known Types Configuration

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: argocd-cm
  namespace: argocd
data:
  # Resource tracking method.
  # Argo CD 3.0 changed the DEFAULT from `label` to `annotation`. On a 2.x->3.x
  # upgrade this silently re-tracks every existing resource. If the FIRST
  # post-upgrade sync also deletes/prunes a resource, the tracking-method change
  # can orphan it. Force a full sync (WITHOUT ApplyOutOfSyncOnly) on all apps
  # right after upgrade, BEFORE any prune/delete, to re-stamp the new tracking
  # identifier. `annotation+label` writes both for compatibility during migration.
  application.resourceTrackingMethod: annotation+label

  # Exclude resources from sync
  resource.exclusions: |
    - apiGroups:
      - "*"
      kinds:
      - ProviderConfigUsage
      clusters:
      - "*"
```

## RBAC Configuration

### AppProject with RBAC

```yaml
apiVersion: argoproj.io/v1alpha1
kind: AppProject
metadata:
  name: team-a
  namespace: argocd
spec:
  description: Team A project

  # Source repositories
  sourceRepos:
    - "https://github.com/team-a/*"
    - "https://charts.team-a.com"

  # Destination clusters and namespaces
  destinations:
    - namespace: "team-a-*"
      server: https://kubernetes.default.svc
    - namespace: team-a-shared
      server: https://prod-cluster.example.com

  # Cluster resource whitelist (what CAN be deployed)
  clusterResourceWhitelist:
    - group: ""
      kind: Namespace
    - group: "rbac.authorization.k8s.io"
      kind: ClusterRole

  # Namespace resource blacklist (what CANNOT be deployed)
  namespaceResourceBlacklist:
    - group: ""
      kind: ResourceQuota
    - group: ""
      kind: LimitRange

  # Roles for project
  roles:
    - name: developer
      description: Developer role
      policies:
        - p, proj:team-a:developer, applications, get, team-a/*, allow
        - p, proj:team-a:developer, applications, sync, team-a/*, allow
      groups:
        - team-a-developers

    - name: admin
      description: Admin role
      policies:
        - p, proj:team-a:admin, applications, *, team-a/*, allow
        - p, proj:team-a:admin, repositories, *, team-a/*, allow
      groups:
        - team-a-admins

  # Orphaned resources monitoring
  orphanedResources:
    warn: true
    ignore:
      - group: ""
        kind: ConfigMap
        name: ignore-this-cm
```

### Global RBAC Policies

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: argocd-rbac-cm
  namespace: argocd
data:
  policy.default: role:readonly

  policy.csv: |
    # Format: p, subject, resource, action, object, effect

    # Grant admin role to group
    g, platform-team, role:admin

    # Custom role: app-deployer
    p, role:app-deployer, applications, get, */*, allow
    p, role:app-deployer, applications, sync, */*, allow
    p, role:app-deployer, applications, override, */*, allow
    p, role:app-deployer, repositories, get, *, allow

    # Grant app-deployer role to groups
    g, deployer-team, role:app-deployer

    # Specific permissions
    p, user:jane@example.com, applications, *, default/*, allow
    p, user:john@example.com, clusters, get, https://prod-cluster, allow

    # Project-scoped permissions
    p, role:project-viewer, applications, get, */*, allow
    p, role:project-viewer, applications, sync, */*, deny

  scopes: "[groups, email]"
```

## Sync Policies and Strategies

### Automated Sync

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: auto-sync-app
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  syncPolicy:
    automated:
      # Auto-sync when Git changes
      prune: true # Remove resources deleted from Git
      selfHeal: true # Revert manual changes to cluster
      allowEmpty: false # Prevent syncing if path is empty

    syncOptions:
      # Create namespace if missing
      - CreateNamespace=true

      # Validate resources before sync
      - Validate=true

      # Use server-side apply (kubectl apply --server-side)
      - ServerSideApply=true

      # Prune resources in foreground
      - PrunePropagationPolicy=foreground

      # Prune resources last (after new resources created)
      - PruneLast=true

      # Replace resource instead of applying
      - Replace=false

      # Respect ignore differences
      - RespectIgnoreDifferences=true

    # Retry policy
    retry:
      limit: 5
      backoff:
        duration: 5s
        factor: 2
        maxDuration: 3m
```

### Manual Sync with Selective Resources

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: manual-sync-app
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp

  # No automated sync policy - manual only
  syncPolicy:
    syncOptions:
      - CreateNamespace=true
      - PruneLast=true

  # Ignore differences for specific resources
  ignoreDifferences:
    - group: apps
      kind: Deployment
      jsonPointers:
        - /spec/replicas
    - group: ""
      kind: Service
      managedFieldsManagers:
        - kube-controller-manager
```

## Secret Management

### Sealed Secrets Integration

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-with-sealed-secrets
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
    # Sealed secrets stored in Git
    # SealedSecret CRD automatically decrypted by controller
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
```

### External Secrets Operator

```yaml
# ExternalSecret in Git repo
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: myapp-secrets
  namespace: myapp
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secrets-manager
    kind: SecretStore
  target:
    name: myapp-secret
    creationPolicy: Owner
  data:
    - secretKey: db-password
      remoteRef:
        key: myapp/production/db
        property: password
```

### ArgoCD Vault Plugin

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-vault
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
    plugin:
      name: argocd-vault-plugin
      env:
        - name: AVP_TYPE
          value: vault
        - name: AVP_AUTH_TYPE
          value: k8s
        - name: AVP_K8S_ROLE
          value: argocd
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp
```

> **The `source.plugin` block only resolves if a matching CMP sidecar exists.** Config Management Plugins are **no longer registered via the `configManagementPlugins` key in `argocd-cm`** — that was deprecated in v2.4 and **completely removed in v2.8**. A plugin now runs as a **sidecar container on `argocd-repo-server`** (plugin config at `/home/argocd/cmp-server/config/plugin.yaml`, entrypoint `/var/run/argocd/argocd-cmp-server`). The Application's `source.plugin.name` selects that sidecar by name; **omit `name` to use auto-discovery** (the sidecar's `discover` rules decide whether it handles the source). See "Config Management Plugins moved to repo-server sidecars" under Expert Practices for the sidecar definition.

### Secrets in Helm Values (Encrypted)

```yaml
# Use SOPS or git-crypt to encrypt values files
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-helm-secrets
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/charts.git
    targetRevision: HEAD
    path: charts/myapp
    helm:
      valueFiles:
        - values.yaml
        # Encrypted with SOPS, decrypted by plugin
        - secrets://values-secrets.yaml
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp
```

## Multi-tenancy Best Practices

### Tenant Isolation with AppProjects

```yaml
apiVersion: argoproj.io/v1alpha1
kind: AppProject
metadata:
  name: tenant-alpha
  namespace: argocd
spec:
  description: Tenant Alpha isolated project

  sourceRepos:
    - "https://github.com/tenant-alpha/*"

  destinations:
    - namespace: "tenant-alpha-*"
      server: https://kubernetes.default.svc

  clusterResourceWhitelist:
    - group: ""
      kind: Namespace

  namespaceResourceWhitelist:
    - group: "*"
      kind: "*"

  namespaceResourceBlacklist:
    - group: ""
      kind: ResourceQuota
    - group: ""
      kind: LimitRange
    - group: "rbac.authorization.k8s.io"
      kind: "*"

  roles:
    - name: tenant-admin
      policies:
        - p, proj:tenant-alpha:tenant-admin, applications, *, tenant-alpha/*, allow
      groups:
        - tenant-alpha-admins
```

### Resource Quotas per Tenant

```yaml
apiVersion: v1
kind: ResourceQuota
metadata:
  name: tenant-alpha-quota
  namespace: tenant-alpha-prod
spec:
  hard:
    requests.cpu: "100"
    requests.memory: 200Gi
    limits.cpu: "200"
    limits.memory: 400Gi
    persistentvolumeclaims: "10"
    services.loadbalancers: "5"
```

### Network Policies for Tenant Isolation

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: tenant-isolation
  namespace: tenant-alpha-prod
spec:
  podSelector: {}
  policyTypes:
    - Ingress
    - Egress
  ingress:
    # Allow from same namespace
    - from:
        - podSelector: {}
    # Allow from ingress controller
    - from:
        - namespaceSelector:
            matchLabels:
              name: ingress-nginx
  egress:
    # Allow to same namespace
    - to:
        - podSelector: {}
    # Allow DNS
    - to:
        - namespaceSelector:
            matchLabels:
              name: kube-system
      ports:
        - protocol: UDP
          port: 53
```

## Progressive Delivery

### Argo Rollouts Integration

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp-rollout
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp-rollout
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
---
apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: myapp
  namespace: myapp
spec:
  replicas: 5
  strategy:
    canary:
      steps:
        - setWeight: 20
        - pause: { duration: 10m }
        - setWeight: 40
        - pause: { duration: 10m }
        - setWeight: 60
        - pause: { duration: 10m }
        - setWeight: 80
        - pause: { duration: 10m }
      analysis:
        templates:
          - templateName: success-rate
        startingStep: 2
      trafficRouting:
        istio:
          virtualService:
            name: myapp-vsvc
            routes:
              - primary
  selector:
    matchLabels:
      app: myapp
  template:
    metadata:
      labels:
        app: myapp
    spec:
      containers:
        - name: myapp
          image: myapp:stable
```

## Monitoring and Observability

### Prometheus Metrics

Each Argo CD component exposes its own metrics on a **different port** — getting the selector/port pairing wrong (a common mistake) scrapes the wrong pod or nothing at all:

| Component                    | Pod label (`app.kubernetes.io/name`) | Metrics port |
| ---------------------------- | ------------------------------------- | ------------ |
| `argocd-application-controller` | `argocd-application-controller`    | `8082`       |
| `argocd-server`              | `argocd-server`                       | `8083`       |
| `argocd-repo-server`         | `argocd-repo-server`                  | `8084`       |

```yaml
# Application-controller metrics (the app sync/health metrics live here on 8082)
apiVersion: v1
kind: Service
metadata:
  name: argocd-application-controller-metrics
  namespace: argocd
  labels:
    app.kubernetes.io/name: argocd-application-controller-metrics
spec:
  ports:
    - name: metrics
      port: 8082
      protocol: TCP
      targetPort: 8082
  selector:
    app.kubernetes.io/name: argocd-application-controller   # NOT argocd-server
---
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: argocd-application-controller-metrics
  namespace: argocd
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: argocd-application-controller-metrics
  endpoints:
    - port: metrics
      interval: 30s
---
# Server metrics on 8083 (API/UI request metrics)
apiVersion: v1
kind: Service
metadata:
  name: argocd-server-metrics
  namespace: argocd
  labels:
    app.kubernetes.io/name: argocd-server-metrics
spec:
  ports:
    - { name: metrics, port: 8083, protocol: TCP, targetPort: 8083 }
  selector:
    app.kubernetes.io/name: argocd-server
---
# Repo-server metrics on 8084 (manifest generation metrics)
apiVersion: v1
kind: Service
metadata:
  name: argocd-repo-server-metrics
  namespace: argocd
  labels:
    app.kubernetes.io/name: argocd-repo-server-metrics
spec:
  ports:
    - { name: metrics, port: 8084, protocol: TCP, targetPort: 8084 }
  selector:
    app.kubernetes.io/name: argocd-repo-server
```

> **PromQL on Argo CD 3.0:** the per-app `argocd_app_sync_status`, `argocd_app_health_status`, and `argocd_app_created_time` metrics were **removed**. Use the labels on `argocd_app_info` instead — e.g. `argocd_app_info{sync_status="OutOfSync"}` or `argocd_app_info{health_status="Degraded"}`. Per-resource health is also no longer persisted by default (see `controller.resource.health.persist`).

### Notification Templates

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: argocd-notifications-cm
  namespace: argocd
data:
  service.slack: |
    token: $slack-token

  template.app-deployed: |
    message: |
      Application {{.app.metadata.name}} is now running new version.
    slack:
      attachments: |
        [{
          "title": "{{ .app.metadata.name}}",
          "title_link":"{{.context.argocdUrl}}/applications/{{.app.metadata.name}}",
          "color": "#18be52",
          "fields": [
          {
            "title": "Sync Status",
            "value": "{{.app.status.sync.status}}",
            "short": true
          },
          {
            "title": "Repository",
            "value": "{{.app.spec.source.repoURL}}",
            "short": true
          }
          ]
        }]

  template.app-health-degraded: |
    message: |
      Application {{.app.metadata.name}} has degraded health.
    slack:
      attachments: |
        [{
          "title": "{{ .app.metadata.name}}",
          "title_link": "{{.context.argocdUrl}}/applications/{{.app.metadata.name}}",
          "color": "#f4c030",
          "fields": [
          {
            "title": "Health Status",
            "value": "{{.app.status.health.status}}",
            "short": true
          },
          {
            "title": "Message",
            "value": "{{.app.status.health.message}}",
            "short": false
          }
          ]
        }]

  trigger.on-deployed: |
    - when: app.status.operationState.phase in ['Succeeded']
      send: [app-deployed]

  trigger.on-health-degraded: |
    - when: app.status.health.status == 'Degraded'
      send: [app-health-degraded]
```

### Application Annotations for Notifications

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp
  namespace: argocd
  annotations:
    notifications.argoproj.io/subscribe.on-deployed.slack: my-channel
    notifications.argoproj.io/subscribe.on-health-degraded.slack: alerts-channel
spec:
  project: default
  source:
    repoURL: https://github.com/myorg/myrepo.git
    targetRevision: HEAD
    path: apps/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp
```

## CLI Operations

### Application Management

```bash
# Create application
argocd app create myapp \
  --repo https://github.com/myorg/myrepo.git \
  --path apps/myapp \
  --dest-server https://kubernetes.default.svc \
  --dest-namespace myapp \
  --sync-policy automated \
  --auto-prune \
  --self-heal

# List applications
argocd app list

# Get application details
argocd app get myapp

# Sync application
argocd app sync myapp

# Sync specific resources
argocd app sync myapp --resource apps:Deployment:myapp

# Rollback to previous version
argocd app rollback myapp

# Delete application
argocd app delete myapp

# Delete application and cascade delete resources
argocd app delete myapp --cascade

# Diff local changes
argocd app diff myapp

# Wait for sync to complete
argocd app wait myapp --health

# Set application parameters
argocd app set myapp --helm-set replicaCount=3
```

### Repository Management

```bash
# Add repository
argocd repo add https://github.com/myorg/myrepo.git \
  --username myuser \
  --password mytoken

# Add private repo with SSH
argocd repo add git@github.com:myorg/myrepo.git \
  --ssh-private-key-path ~/.ssh/id_rsa

# List repositories
argocd repo list

# Remove repository
argocd repo rm https://github.com/myorg/myrepo.git
```

### Cluster Management

```bash
# Add cluster
argocd cluster add my-cluster-context

# List clusters
argocd cluster list

# Remove cluster
argocd cluster rm https://my-cluster.example.com
```

### Project Management

```bash
# Create project
argocd proj create myproject \
  --description "My project" \
  --src https://github.com/myorg/* \
  --dest https://kubernetes.default.svc,myapp-*

# Add role to project
argocd proj role create myproject developer

# Add policy to role
argocd proj role add-policy myproject developer \
  --action get --permission allow \
  --object 'applications'

# List projects
argocd proj list

# Get project details
argocd proj get myproject
```

## Best Practices

### Repository Organization

1. **Separate config from code**: Keep application code and Kubernetes manifests in separate repositories
2. **Environment branches or directories**: Use either branch-per-environment or directory-per-environment strategy
3. **Immutable tags**: Use Git commit SHAs or immutable tags for production deployments
4. **PR-based deployments**: Require pull requests for changes to production manifests

### Application Design

1. **One app per microservice**: Create separate Argo CD applications for each microservice
2. **Use AppProjects**: Group related applications and enforce RBAC boundaries
3. **Implement sync waves**: Order resource creation with sync waves and hooks
4. **Health checks**: Define custom health checks for CRDs and custom resources
5. **Resource limits**: Always define resource requests and limits

### Security

1. **Least privilege RBAC**: Grant minimum necessary permissions per team/project
2. **Encrypted secrets**: Never commit plain-text secrets to Git
3. **Separate credentials**: Use different Git credentials for different environments
4. **Audit logging**: Enable and monitor Argo CD audit logs
5. **Network policies**: Restrict network access to Argo CD components

### Sync Strategies

1. **Automated sync for non-prod**: Enable auto-sync and self-heal for dev/staging
2. **Manual sync for production**: Require manual approval for production syncs
3. **Prune with caution**: Use prune: true carefully, consider PruneLast option
4. **Sync windows**: Configure sync windows to prevent deployments during business hours
5. **Progressive rollouts**: Use Argo Rollouts for canary and blue-green deployments

### Multi-cluster Management

1. **Cluster naming**: Use consistent naming conventions for clusters
2. **Cluster labels**: Label clusters by environment, region, purpose
3. **ApplicationSets**: Use ApplicationSets to manage apps across clusters
4. **Cluster secrets**: Rotate cluster credentials regularly
5. **Disaster recovery**: Maintain Argo CD configuration in Git for easy recovery

### Observability

1. **Metrics**: Export Prometheus metrics and create dashboards
2. **Notifications**: Configure notifications for sync failures and health degradation
3. **Logging**: Centralize Argo CD logs for troubleshooting
4. **Tracing**: Enable distributed tracing for complex deployments
5. **Alerts**: Set up alerts for out-of-sync applications

### Performance

1. **Resource limits**: Set appropriate resource limits for Argo CD components
2. **Sharding**: Use controller sharding for large-scale deployments (1000+ apps)
3. **Cache optimization**: Configure Redis for improved performance
4. **Webhook-based sync**: Use Git webhooks instead of polling for faster syncs
5. **Selective sync**: Use resource inclusions/exclusions to reduce sync scope

### Disaster Recovery

1. **Backup configuration**: Store all Argo CD configuration in Git
2. **Multiple Argo CD instances**: Run separate instances for different environments
3. **Export applications**: Regularly export application definitions
4. **Document procedures**: Maintain runbooks for disaster recovery
5. **Test recovery**: Periodically test disaster recovery procedures

## Troubleshooting

### Common Issues

#### Application stuck in progressing state

```bash
# Check application status
argocd app get myapp

# Check sync status and health
kubectl get application myapp -n argocd -o yaml

# Manual sync with replace
argocd app sync myapp --replace
```

#### Out of sync despite no changes

```bash
# Hard refresh
argocd app get myapp --hard-refresh

# Check for ignored differences
argocd app diff myapp
```

#### Permission denied errors

```bash
# Check project permissions
argocd proj get myproject

# Verify RBAC policies
kubectl get cm argocd-rbac-cm -n argocd -o yaml
```

#### Sync fails with validation errors

```bash
# Skip validation
argocd app sync myapp --validate=false

# Or add to syncOptions
syncOptions:
  - Validate=false
```

### Debug Commands

```bash
# Enable debug logging
argocd app sync myapp --loglevel debug

# Get application events
kubectl get events -n argocd --field-selector involvedObject.name=myapp

# Check controller logs
kubectl logs -n argocd deployment/argocd-application-controller

# Check server logs
kubectl logs -n argocd deployment/argocd-server

# Get resource details
argocd app resources myapp
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

Hard-won, mechanism-level guidance. Each item explains *why*, not just *what* — that is what separates a working manifest from one that silently misbehaves in production.

### Sync & Diffing Gotchas

**`ignoreDifferences` alone is cosmetic during sync — pair it with `RespectIgnoreDifferences=true`.** By default `ignoreDifferences` only affects **drift detection** (what shows as OutOfSync); it does **not** affect the sync patch, so on the next sync Argo CD resets the ignored fields back to the Git values. `RespectIgnoreDifferences=true` makes Argo CD "consider the configurations made in the `spec.ignoreDifferences` attribute also during the sync stage." Critical limitation: it only works when the resource **already exists** — on initial creation with no live state, the desired state is applied as-is. Essential for HPA-managed replicas, sidecar-injecting mutating webhooks, and cert-manager-injected `caBundle` fields.

```yaml
spec:
  ignoreDifferences:
    - group: apps
      kind: Deployment
      jsonPointers:
        - /spec/replicas                       # HPA manages this
    - group: admissionregistration.k8s.io
      kind: MutatingWebhookConfiguration
      jqPathExpressions:
        - .webhooks[].clientConfig.caBundle     # cert-manager injects this
  syncPolicy:
    syncOptions:
      - RespectIgnoreDifferences=true           # else ignored fields get overwritten on sync
```

**Mutating webhooks/controllers cause perpetual OutOfSync.** Sidecar injection, HPA/VPA changing replicas/requests, cloud controllers populating `status.loadBalancer`, and Kubernetes normalizing quantities (`1000m`->`1`, `3072Mi`->`3Gi`) all mutate resources *after* apply: each sync succeeds, then live state diverges immediately, looping forever. Fix by targeting the mutated fields with `ignoreDifferences` (use `jqPathExpressions` or `managedFieldsManagers`) **and** adding `RespectIgnoreDifferences=true`, ideally with `ServerSideApply=true` for explicit field-ownership tracking.

```yaml
ignoreDifferences:
  - group: apps
    kind: Deployment
    jsonPointers:
      - /spec/replicas                                # HPA
    jqPathExpressions:
      - .spec.template.spec.containers[].resources    # VPA
syncPolicy:
  syncOptions:
    - RespectIgnoreDifferences=true
    - ServerSideApply=true
```

**Automated sync does not retry a failed commit-SHA — `selfHeal` is what re-triggers it.** Automated sync attempts exactly one synchronization per unique (commit-SHA + parameters): "Automatic sync will not reattempt a sync if the previous sync attempt against the same commit-SHA and parameters had failed." A failed SHA effectively "sticks" until a new commit arrives or `selfHeal` detects live-state drift (re-attempts after the self-heal timeout, 5s by default). This silently breaks pipelines that push one commit and expect Argo CD to keep trying. Enable `selfHeal` for drift-based re-triggering and set `syncPolicy.retry` for transient in-attempt errors.

```yaml
syncPolicy:
  automated:
    prune: true
    selfHeal: true        # re-triggers on live-state drift, bypassing the SHA dedup
  retry:
    limit: 5
    backoff: { duration: 5s, factor: 2, maxDuration: 3m }
```

**Self-managed Argo CD requires `ServerSideApply=true`; never combine it with `Replace=true`.** Client-side apply stores prior desired state in the `kubectl.kubernetes.io/last-applied-configuration` annotation (~262KB cap); large CRDs (the ApplicationSet CRD now exceeds this) overflow it, and managing Argo CD with itself hits field-ownership conflicts when fields were originally set by Helm/kubectl. Use `ServerSideApply=true` so the Argo CD field manager owns fields (`kubectl apply --server-side --force-conflicts` during manual migration; `ClientSideApplyMigration` assists transferring `managedFields`). Do **not** also set `Replace=true` — "Replace=true takes precedence over ServerSideApply=true", so SSA is silently skipped.

```yaml
spec:
  syncPolicy:
    syncOptions:
      - ServerSideApply=true
      # do NOT also add Replace=true — it overrides SSA
```

### Hooks & Ordering Gotchas

**Resource hooks are skipped during a selective sync; failed `PreDelete` hooks block Application deletion.** All hooks (PreSync/Sync/PostSync/SyncFail) are entirely skipped during `argocd app sync myapp --resource ...` — "hooks do not run during a selective sync operation." So a `PreSync` db-migration silently does **not** run when someone selectively syncs only the Deployment, risking schema/data mismatch. Separately, a failed `PreDelete` hook blocks the *whole* Application deletion until it succeeds or is manually removed — give delete-time hook Jobs a `backoffLimit` and `activeDeadlineSeconds` so a failing hook cannot block deletion forever, and keep failed `PreSync` Jobs (`HookFailed` delete policy) for diagnosis.

```yaml
metadata:
  annotations:
    argocd.argoproj.io/hook: PreDelete
    argocd.argoproj.io/hook-delete-policy: HookSucceeded
spec:
  backoffLimit: 2
  activeDeadlineSeconds: 120   # bounded so a failing hook can't block delete forever
```

**Sync waves order resources within ONE Application; for cross-Application ordering use Progressive Syncs.** `argocd.argoproj.io/sync-wave` orders resources within a single Application's sync (Argo applies a wave, waits for Healthy, advances). It does **not** reliably sequence sibling Applications in an app-of-apps on ongoing syncs — wave annotations on child Application objects only help during the parent's *initial* creation. The root cause: the built-in health check for the `argoproj.io/Application` CRD was removed in 1.8, so a wave advances as soon as the child Application *object* is applied, not when its workloads are Healthy. For reliable cross-Application sequencing use ApplicationSet Progressive Syncs (RollingSync).

**Restore the Application CRD health check in `argocd-cm` for app-of-apps wave ordering.** Because the `argoproj.io/Application` health assessment was removed in 1.8, restore it with a Lua customization that surfaces `obj.status.health.status`. This is invisible in the UI — the wave appears to progress correctly while pods are still initializing.

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: argocd-cm
  namespace: argocd
data:
  resource.customizations.health.argoproj.io_Application: |
    hs = {}
    hs.status = "Progressing"
    hs.message = ""
    if obj.status ~= nil then
      if obj.status.health ~= nil then
        hs.status = obj.status.health.status
        if obj.status.health.message ~= nil then
          hs.message = obj.status.health.message
        end
      end
    end
    return hs
```

### Deletion & Finalizer Gotchas

**Cascading deletion of managed resources requires the `resources-finalizer` — never add it reflexively to app-of-apps roots.** Deleting an Application does **not** delete the Kubernetes resources it manages unless `resources-finalizer.argocd.argoproj.io` is present; without it, deleting the Application *orphans* its Deployments/Services. The inverse footgun: adding the finalizer to an app-of-apps **root** means a single `kubectl delete` of the root cascades through every child Application and wipes the whole environment. Add it deliberately (e.g. ephemeral PR previews), not reflexively on production roots.

```yaml
# Intentional cascade for an ephemeral environment ONLY:
metadata:
  name: pr-preview-123
  finalizers:
    - resources-finalizer.argocd.argoproj.io
```

### Helm Idioms & Gotchas

**Argo CD runs `helm template`, not `helm install` — and any Argo hook in a chart disables ALL Helm hooks.** Argo CD uses Helm only to inflate manifests; "the lifecycle of the application is handled by Argo CD instead of Helm." Consequences: `helm ls`/`history` show nothing, `helm rollback` is unavailable, `helm test` is not run. Helm hooks map to Argo CD phases, but: "If you define any Argo CD hooks, all Helm hooks will be ignored." Never mix the two hook systems in one chart — pick Argo CD hooks for Argo-managed charts. Also, overriding `releaseName` breaks the `app.kubernetes.io/instance` label (Argo injects it with the Application name), which can break label selectors.

```yaml
# Argo CD hooks for Argo-managed charts (do NOT also use helm.sh/hook):
metadata:
  annotations:
    argocd.argoproj.io/hook: PostSync
    argocd.argoproj.io/hook-delete-policy: HookSucceeded
```

**Helm `valueFiles` precedence is last-wins.** "When multiple valueFiles are specified, the last file listed has the highest precedence" — the *opposite* of first-match-wins systems, so put base values first and environment overrides last. Full documented precedence (lowest to highest): chart `values.yaml` < `valueFiles` (last wins) < `values` (inline string) < `valuesObject` < `parameters`. Glob-matched `valueFiles` expand in lexical order, so use numeric filename prefixes when override order matters.

```yaml
helm:
  valueFiles:
    - values.yaml             # base defaults (lowest)
    - values-production.yaml  # overrides win because listed last
```

### ApplicationSet Gotchas

**`applicationsSync` policies do NOT stop cascade deletion — add a finalizer (and beware the global `--policy` override).** `create-only`/`create-update` only govern the controller's *modify* operations; they do not stop child Applications being deleted when the ApplicationSet is deleted — "It doesn't prevent Application controller from deleting Applications according to ownerReferences." To prevent that, add the `resources-finalizer.argocd.argoproj.io` finalizer to the ApplicationSet (plus `preserveResourcesOnDeletion: true` to keep the children's cluster resources). Also, the controller-level `--policy` flag **overrides** per-ApplicationSet `applicationsSync` unless per-set override is enabled (`ARGOCD_APPLICATIONSET_CONTROLLER_ENABLE_POLICY_OVERRIDE` / `--enable-policy-override`, default false) — so per-set settings can be silently ignored.

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata:
  finalizers:
    - resources-finalizer.argocd.argoproj.io   # stops cascade-delete of child Applications
spec:
  syncPolicy:
    applicationsSync: create-update
    preserveResourcesOnDeletion: true
```

**Progressive Syncs (RollingSync) force-disable autosync on every generated Application.** With `strategy: RollingSync`, "RollingSync will force all generated Applications to have autosync disabled" (warnings are logged for any app that still declares automated sync). The controller sequences syncs itself, so trigger at the step level, not per app. Applications not selected by any step expression are **excluded** and must be synced manually. Progressive Syncs reached Beta in v3.3.0 but is still **behind a feature flag** — enable it explicitly (controller arg/env/configmap). Ensure step `matchExpressions` align with the labels your template actually sets.

```yaml
spec:
  strategy:
    type: RollingSync
    rollingSync:
      steps:
        - matchExpressions: [{ key: env, operator: In, values: [staging] }]
        - matchExpressions: [{ key: env, operator: In, values: [production] }]
          maxUpdate: 25%
  template:
    metadata:
      labels:
        env: '{{env}}'   # must match the step selectors
```

**Use `ignoreApplicationDifferences` so the controller stops reverting per-app overrides.** By default the ApplicationSet controller reverts any field on a generated Application that diverges from the template (including `syncPolicy`) within seconds — so you cannot disable auto-sync on one app during an incident. `spec.ignoreApplicationDifferences` lets you list `jsonPointers`/`jqPathExpressions` the controller leaves alone (commonly `/spec/syncPolicy`). Limitation: it uses MergePatch, so "existing lists will be completely replaced by new lists" — ignoring a field inside a list breaks if any other element changes; target the specific element with a JQ path. (Distinct from per-resource `ignoreDifferences`, which controls drift detection against the cluster.)

```yaml
spec:
  ignoreApplicationDifferences:
    - jsonPointers:
        - /spec/syncPolicy   # allow per-app auto-sync toggles during incidents
    - name: prod-app
      jsonPointers:
        - /spec/source/targetRevision
```

**Go-template ApplicationSets produce empty strings for missing keys unless `goTemplateOptions: missingkey=error`.** In Go-template mode an undefined generator key (e.g. a typo `{{.server}}`->`{{.srevr}}`) renders as an empty string, silently producing Applications with blank destinations/namespaces and deploying to the wrong place. Set `goTemplateOptions: ['missingkey=error']` so undefined-key access fails the render. Applies only to `goTemplate: true`; the legacy fasttemplate mode uses `{{value}}` without a dot.

```yaml
spec:
  goTemplate: true
  goTemplateOptions:
    - missingkey=error    # typos fail loudly instead of deploying to blank/wrong targets
  template:
    spec:
      destination:
        server: '{{.server}}'
```

### Security

**`policy.default` grants ALL authenticated users a baseline that deny rules cannot revoke.** Every authenticated user gets at least the permissions in `policy.default`, and the docs are explicit: "All authenticated users get at least the permissions granted by the default policies. This access cannot be blocked by a deny rule." So `policy.default: role:readonly` plus per-user deny rules to claw it back is silently ineffective. Safe baseline: leave `policy.default` **empty/unset** (no default grant) and explicitly grant least-privilege roles per group. Note `deny` otherwise wins over `allow` at equal scope, but it cannot override the default grant.

```yaml
# argocd-rbac-cm: no default permissions; grant explicitly per group
data:
  policy.default: ''        # empty: no baseline grant for authenticated users
  scopes: '[groups, email]'
  policy.csv: |
    g, platform-team, role:readonly
    g, deployers, role:app-deployer
```

**Never template the ApplicationSet `project` field; SCM/PR generators are admin-only.** If `spec.template.spec.project` is templated from generator output (e.g. `{{path.basename}}`), anyone who can write to the generator's source of truth can steer generated Applications into a privileged project (even `default`) and escalate: "If the project field is not hard-coded in an ApplicationSet's template, then admins must control all sources of truth for the ApplicationSet's generators." SCM Provider and Pull Request generators are admin-only — "Only admins may create ApplicationSets to avoid leaking Secrets." Safe pattern: hard-code `project`, restrict generator sources, restrict ApplicationSet creation to admins.

```yaml
spec:
  generators:
    - git:
        directories:
          - path: teams/*
  template:
    spec:
      project: platform-team   # hard-coded — cannot be influenced by repo content
      # NOT: project: '{{path.basename}}'  # attacker creates a 'default' dir to escape
```

### Currency: Version-Breaking Changes

**Config Management Plugins in `argocd-cm` were removed in v2.8 — use repo-server sidecars.** The legacy `configManagementPlugins` key was deprecated in v2.4 and "completely removed starting in v2.8." Plugins now run as **sidecar containers on `argocd-repo-server`** (config at `/home/argocd/cmp-server/config/plugin.yaml`, entrypoint `/var/run/argocd/argocd-cmp-server`). Auto-discovery works when the Application's `plugin` block omits `name`. This is more secure (isolated container, separate image).

```yaml
# argocd-repo-server: add a CMP sidecar
containers:
  - name: avp
    image: my-avp-image:latest
    command: [/var/run/argocd/argocd-cmp-server]
    volumeMounts:
      - { name: var-files, mountPath: /var/run/argocd }
      - { name: plugins, mountPath: /home/argocd/cmp-server/plugins }
      - { name: plugin-config, mountPath: /home/argocd/cmp-server/config/plugin.yaml, subPath: plugin.yaml }
```

**Argo CD 3.0 changed many defaults — it is a high-blast-radius upgrade.** From the 2.14->3.0 upgrade notes:

1. **Resource tracking** changed from label to **annotation** ("changed to use annotation-based tracking"); opt out with `application.resourceTrackingMethod: label`. Switching tracking mid-lifecycle risks orphaning a resource if the first post-upgrade sync also deletes one — force a full sync before any prune.
2. **RBAC sub-resource inheritance removed**: "update or delete actions only apply to the application itself; new policies must be defined to allow `update/*` or `delete/*`", and `logs, get` is now enforced by default.
3. **`repositories`/`repository.credentials` in `argocd-cm` removed** ("no longer available in Argo CD 3.0") — migrate to Secrets labeled `argocd.argoproj.io/secret-type: repository`.
4. **Metrics removed**: `argocd_app_sync_status`, `argocd_app_health_status`, `argocd_app_created_time` — use labels on `argocd_app_info`; per-resource health is no longer persisted by default (`controller.resource.health.persist`).

```yaml
# RBAC after 3.0 — sub-resource and logs grants are now EXPLICIT:
p, role:app-deployer, applications, sync, */*, allow
p, role:app-deployer, applications, update/*, */*, allow
p, role:app-deployer, applications, delete/*, */*, allow
p, role:app-deployer, logs, get, */*, allow
# PromQL: argocd_app_info{sync_status="OutOfSync"}  (not argocd_app_sync_status)
```

## References

- [Argo CD Documentation](https://argo-cd.readthedocs.io/)
- [Argo CD Best Practices](https://argo-cd.readthedocs.io/en/stable/user-guide/best_practices/)
- [GitOps Principles](https://opengitops.dev/)
- [Argo Rollouts](https://argoproj.github.io/argo-rollouts/)
