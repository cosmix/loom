---
name: kubernetes
description: Kubernetes deployment, cluster architecture, security, and operations. Includes manifests, Helm charts, RBAC, network policies, troubleshooting, and production best practices. Trigger keywords: kubernetes, k8s, pod, deployment, statefulset, daemonset, service, ingress, configmap, secret, pvc, namespace, helm, chart, kubectl, cluster, rbac, networkpolicy, podsecurity, operator, crd, job, cronjob, hpa, pdb, kustomize.
allowed-tools: Read, Grep, Glob, Edit, Write, Bash
---

# Kubernetes

## Overview

This skill covers Kubernetes resource configuration, deployment strategies, cluster architecture, security hardening, Helm chart development, and production operations. It helps create production-ready manifests, troubleshoot cluster issues, and implement security best practices.

## Agent Delegation

**software-engineer** (Sonnet) - Writes K8s manifests, implements configs
**senior-software-engineer** (Opus) - Application architecture on K8s, multi-service design
**security-engineer** (Opus) - RBAC, NetworkPolicies, PodSecurityStandards, admission control
**senior-infrastructure-engineer** (Opus) - Cluster architecture AND implementation, operators, CRDs

## Instructions

### 1. Design Resource Architecture

- Plan namespace organization and boundaries
- Define resource requests/limits based on workload
- Design service mesh topology (if applicable)
- Plan for high availability (replicas, affinity, PDBs)
- Choose workload type (Deployment/StatefulSet/DaemonSet/Job)
- Design storage strategy (PVCs, StorageClasses, ephemeral volumes)

### 2. Create Resource Manifests

- Write Deployments/StatefulSets/DaemonSets with proper lifecycle
- Configure Services (ClusterIP/NodePort/LoadBalancer) and Ingress
- Set up ConfigMaps and Secrets (sealed secrets, external secrets)
- Define RBAC policies (ServiceAccounts, Roles, RoleBindings)
- Implement NetworkPolicies for pod-to-pod communication
- Configure PodDisruptionBudgets for availability guarantees

### 3. Develop Helm Charts

- Structure charts (Chart.yaml, values.yaml, templates/)
- Use template functions (\_helpers.tpl for common labels)
- Parameterize configurations (replicas, image tags, resources)
- Define dependencies (requirements.yaml or Chart.yaml dependencies)
- Implement hooks (pre-install, post-upgrade, etc.)
- Test charts locally (helm template, helm lint, helm install --dry-run)
- Package and publish charts to registries

### 4. Implement Security Best Practices

- Run as non-root user (runAsUser, runAsNonRoot)
- Drop all capabilities (securityContext.capabilities.drop: [ALL])
- Use read-only root filesystems (readOnlyRootFilesystem: true)
- Prevent privilege escalation (allowPrivilegeEscalation: false)
- Implement PodSecurityStandards (restricted profile)
- Use NetworkPolicies for zero-trust networking
- Scan images for vulnerabilities (integrate Trivy, Snyk)
- Rotate secrets regularly (external secrets operator)
- Audit RBAC permissions (principle of least privilege)

### 5. Configure Observability

- Add Prometheus annotations for scraping
- Configure logging (stdout/stderr, fluentd/loki)
- Implement health probes (liveness, readiness, startup)
- Export metrics (custom metrics for HPA)
- Set up distributed tracing (OpenTelemetry)
- Configure alerting rules

### 6. Troubleshoot Issues

- Pod crashes: `kubectl logs`, `kubectl describe pod`, events
- Image pull failures: Check ImagePullSecrets, registry auth
- Networking: Test DNS resolution, check NetworkPolicies
- Resource constraints: Check node capacity, pod eviction
- Scheduling failures: Node selectors, taints/tolerations, affinity
- Performance: Use `kubectl top`, Prometheus metrics
- CrashLoopBackOff: Check startup probes, init containers
- Pending pods: Describe pod for scheduling failures

### 7. Production Operations

- Rolling updates with proper rollout strategy
- Blue-green deployments (multiple services)
- Canary deployments (traffic splitting via service mesh)
- Backup and restore (Velero for cluster state)
- Disaster recovery planning (multi-region, etcd backups)
- Capacity planning (resource quotas, limit ranges)
- Cost optimization (right-sizing, spot instances, cluster autoscaler)
- Upgrade planning (test in staging, rolling node upgrades)

## Best Practices

1. **Use Namespaces**: Organize resources logically, enable RBAC boundaries
2. **Set Resource Limits**: Prevent resource exhaustion, enable QoS classes
3. **Health Probes**: Configure liveness, readiness, and startup probes
4. **Rolling Updates**: Zero-downtime deployments with maxSurge/maxUnavailable
5. **Secrets Management**: Use external secrets, never hardcode credentials
6. **Label Everything**: Enable filtering, selection, and monitoring
7. **Use Helm/Kustomize**: Template and manage manifests as code
8. **Security Contexts**: Always run as non-root with minimal privileges
9. **NetworkPolicies**: Default deny, explicit allow for zero-trust
10. **Pod Disruption Budgets**: Protect availability during disruptions
11. **Resource Quotas**: Prevent runaway resource consumption per namespace
12. **Admission Control**: Use OPA/Kyverno for policy enforcement
13. **Immutable Infrastructure**: Never SSH into pods, replace instead
14. **GitOps**: Use ArgoCD/Flux for declarative deployment

## Examples

### Example 1: Production Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-server
  namespace: production
  labels:
    app: api-server
    version: v1.2.0
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: api-server
  template:
    metadata:
      labels:
        app: api-server
        version: v1.2.0
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
    spec:
      serviceAccountName: api-server
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000

      containers:
        - name: api
          image: myregistry.io/api-server:v1.2.0
          imagePullPolicy: IfNotPresent

          ports:
            - name: http
              containerPort: 8080
              protocol: TCP

          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: api-secrets
                  key: database-url
            - name: LOG_LEVEL
              valueFrom:
                configMapKeyRef:
                  name: api-config
                  key: log-level

          resources:
            requests:
              cpu: 100m
              memory: 128Mi
            limits:
              cpu: 500m
              memory: 512Mi

          livenessProbe:
            httpGet:
              path: /health/live
              port: http
            initialDelaySeconds: 15
            periodSeconds: 20
            timeoutSeconds: 5
            failureThreshold: 3

          readinessProbe:
            httpGet:
              path: /health/ready
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
            timeoutSeconds: 3
            failureThreshold: 3

          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop:
                - ALL

          volumeMounts:
            - name: tmp
              mountPath: /tmp
            - name: cache
              mountPath: /app/cache

      volumes:
        - name: tmp
          emptyDir: {}
        - name: cache
          emptyDir:
            sizeLimit: 100Mi

      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
            - weight: 100
              podAffinityTerm:
                labelSelector:
                  matchLabels:
                    app: api-server
                topologyKey: kubernetes.io/hostname

      topologySpreadConstraints:
        - maxSkew: 1
          topologyKey: topology.kubernetes.io/zone
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels:
              app: api-server
```

### Example 2: Service and Ingress

```yaml
apiVersion: v1
kind: Service
metadata:
  name: api-server
  namespace: production
  labels:
    app: api-server
spec:
  type: ClusterIP
  ports:
    - port: 80
      targetPort: http
      protocol: TCP
      name: http
  selector:
    app: api-server

---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: api-server
  namespace: production
  annotations:
    kubernetes.io/ingress.class: nginx
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/rate-limit: "100"
    nginx.ingress.kubernetes.io/rate-limit-window: "1m"
spec:
  tls:
    - hosts:
        - api.example.com
      secretName: api-tls-cert
  rules:
    - host: api.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: api-server
                port:
                  number: 80
```

### Example 3: ConfigMap and Secret

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: api-config
  namespace: production
data:
  log-level: "info"
  max-connections: "100"
  cache-ttl: "3600"
  feature-flags: |
    {
      "new-checkout": true,
      "beta-features": false
    }

---
apiVersion: v1
kind: Secret
metadata:
  name: api-secrets
  namespace: production
type: Opaque
stringData:
  database-url: "postgresql://user:password@db-host:5432/myapp"
  api-key: "super-secret-key"
```

### Example 4: Horizontal Pod Autoscaler

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: api-server
  namespace: production
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: api-server
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
          value: 10
          periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 0
      policies:
        - type: Percent
          value: 100
          periodSeconds: 15
        - type: Pods
          value: 4
          periodSeconds: 15
      selectPolicy: Max
```

### Example 5: Helm Chart Structure

```text
mychart/
  Chart.yaml          # Chart metadata
  values.yaml         # Default configuration values
  templates/
    _helpers.tpl      # Template helpers
    deployment.yaml   # Deployment template
    service.yaml      # Service template
    ingress.yaml      # Ingress template
    configmap.yaml    # ConfigMap template
    secret.yaml       # Secret template
    NOTES.txt         # Post-install notes
  charts/             # Chart dependencies
  .helmignore         # Files to ignore
```

Chart.yaml:

```yaml
apiVersion: v2
name: api-server
description: Production API server Helm chart
type: application
version: 1.2.0
appVersion: "1.2.0"
keywords:
  - api
  - backend
maintainers:
  - name: Platform Team
    email: platform@example.com
dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: "https://charts.bitnami.com/bitnami"
    condition: postgresql.enabled
```

values.yaml:

```yaml
replicaCount: 3

image:
  repository: myregistry.io/api-server
  tag: "1.2.0"
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80
  targetPort: 8080

ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
  hosts:
    - host: api.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: api-tls-cert
      hosts:
        - api.example.com

resources:
  requests:
    cpu: 100m
    memory: 128Mi
  limits:
    cpu: 500m
    memory: 512Mi

autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80

postgresql:
  enabled: true
  auth:
    username: apiuser
    database: apidb
```

templates/\_helpers.tpl:

```yaml
{{- define "api-server.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "api-server.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "api-server.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
app.kubernetes.io/name: {{ include "api-server.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}
```

### Example 6: NetworkPolicy (Zero-Trust)

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: api-server-netpol
  namespace: production
spec:
  podSelector:
    matchLabels:
      app: api-server
  policyTypes:
    - Ingress
    - Egress

  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              name: ingress-nginx
        - podSelector:
            matchLabels:
              app: frontend
      ports:
        - protocol: TCP
          port: 8080

  egress:
    - to:
        - podSelector:
            matchLabels:
              app: postgresql
      ports:
        - protocol: TCP
          port: 5432

    - to:
        - podSelector:
            matchLabels:
              k8s-app: kube-dns
          namespaceSelector:
            matchLabels:
              name: kube-system
      ports:
        - protocol: UDP
          port: 53

    - to:
        - namespaceSelector: {}
      ports:
        - protocol: TCP
          port: 443

---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: default-deny-all
  namespace: production
spec:
  podSelector: {}
  policyTypes:
    - Ingress
    - Egress
```

### Example 7: RBAC Configuration

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: api-server
  namespace: production
  labels:
    app: api-server

---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: api-server
  namespace: production
rules:
  - apiGroups: [""]
    resources: ["configmaps"]
    verbs: ["get", "list", "watch"]

  - apiGroups: [""]
    resources: ["secrets"]
    resourceNames: ["api-secrets", "database-creds"]
    verbs: ["get"]

  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list"]

---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: api-server
  namespace: production
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: Role
  name: api-server
subjects:
  - kind: ServiceAccount
    name: api-server
    namespace: production

---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: pod-reader
rules:
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list", "watch"]

---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: read-pods-global
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: pod-reader
subjects:
  - kind: ServiceAccount
    name: monitoring
    namespace: observability
```

### Example 8: StatefulSet with Persistent Storage

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgresql
  namespace: production
spec:
  serviceName: postgresql-headless
  replicas: 3
  selector:
    matchLabels:
      app: postgresql

  template:
    metadata:
      labels:
        app: postgresql
    spec:
      containers:
        - name: postgres
          image: postgres:15-alpine
          ports:
            - containerPort: 5432
              name: postgres
          env:
            - name: POSTGRES_DB
              value: myapp
            - name: POSTGRES_USER
              valueFrom:
                secretKeyRef:
                  name: postgres-creds
                  key: username
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: postgres-creds
                  key: password
            - name: PGDATA
              value: /var/lib/postgresql/data/pgdata
          volumeMounts:
            - name: data
              mountPath: /var/lib/postgresql/data
          resources:
            requests:
              cpu: 250m
              memory: 512Mi
            limits:
              cpu: 1000m
              memory: 2Gi

  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        accessModes: ["ReadWriteOnce"]
        storageClassName: fast-ssd
        resources:
          requests:
            storage: 50Gi

---
apiVersion: v1
kind: Service
metadata:
  name: postgresql-headless
  namespace: production
spec:
  clusterIP: None
  selector:
    app: postgresql
  ports:
    - port: 5432
      targetPort: postgres
      name: postgres
```

## Troubleshooting Commands

```bash
# Pod debugging
kubectl get pods -n production
kubectl describe pod api-server-abc123 -n production
kubectl logs api-server-abc123 -n production
kubectl logs api-server-abc123 -n production --previous
kubectl logs api-server-abc123 -c container-name -n production
kubectl exec -it api-server-abc123 -n production -- /bin/sh

# Events and status
kubectl get events -n production --sort-by='.lastTimestamp'
kubectl get events -n production --field-selector involvedObject.name=api-server-abc123

# Resource usage
kubectl top nodes
kubectl top pods -n production
kubectl describe node node-1

# Network debugging
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash
kubectl exec -it api-server-abc123 -n production -- nslookup kubernetes.default
kubectl exec -it api-server-abc123 -n production -- curl -v http://other-service

# Configuration
kubectl get configmap api-config -n production -o yaml
kubectl get secret api-secrets -n production -o jsonpath='{.data}'

# Deployments and rollouts
kubectl rollout status deployment/api-server -n production
kubectl rollout history deployment/api-server -n production
kubectl rollout undo deployment/api-server -n production
kubectl rollout restart deployment/api-server -n production

# RBAC debugging
kubectl auth can-i get pods --as=system:serviceaccount:production:api-server -n production
kubectl get rolebindings,clusterrolebindings --all-namespaces -o json | jq '.items[] | select(.subjects[]?.name=="api-server")'

# NetworkPolicy debugging
kubectl describe networkpolicy api-server-netpol -n production

# Helm operations
helm list -n production
helm get values api-server -n production
helm history api-server -n production
helm upgrade api-server ./mychart -n production --values values.yaml
helm rollback api-server 2 -n production
helm uninstall api-server -n production

# Cluster info
kubectl cluster-info
kubectl get nodes -o wide
kubectl get componentstatuses
kubectl api-resources
kubectl api-versions
```

## Common Issues and Solutions

| Issue                   | Cause                                            | Solution                                           |
| ----------------------- | ------------------------------------------------ | -------------------------------------------------- |
| ImagePullBackOff        | Registry auth missing or image not found         | Check imagePullSecrets, verify image exists        |
| CrashLoopBackOff        | App crashes on startup                           | Check logs, verify config, add startup probe       |
| Pending pod             | Insufficient resources or scheduling constraints | Check node capacity, taints, tolerations, affinity |
| OOMKilled               | Memory limit exceeded                            | Increase memory limit, fix memory leak             |
| Service unreachable     | Wrong selector or port                           | Verify selector matches pod labels, check ports    |
| DNS resolution fails    | CoreDNS issues or NetworkPolicy blocking         | Check CoreDNS pods, verify NetworkPolicy egress    |
| PVC pending             | StorageClass missing or no available volumes     | Verify StorageClass, check provisioner             |
| RBAC permission denied  | ServiceAccount lacks required permissions        | Add Role/RoleBinding with necessary verbs          |
| Readiness probe failing | App not ready or probe misconfigured             | Adjust initialDelaySeconds, check probe endpoint   |
| HPA not scaling         | Metrics server missing or no resource requests   | Install metrics-server, set resource requests      |
