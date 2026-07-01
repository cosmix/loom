---
name: loom-istio
description: Service mesh implementation with Istio for microservices traffic management, security, and observability. Use for mTLS, traffic routing, load balancing, circuit breakers, retries, timeouts, canary/blue-green deployments, A/B testing, and Envoy sidecar configuration.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - istio
  - service mesh
  - envoy
  - sidecar
  - virtualservice
  - destinationrule
  - gateway
  - mtls
  - peerauthentication
  - authorizationpolicy
  - serviceentry
  - traffic management
  - traffic splitting
  - canary
  - blue-green
  - circuit breaker
  - retry
  - timeout
  - load balancing
  - ingress
  - egress
  - observability
  - tracing
  - telemetry
---

# Istio Service Mesh

## Overview

Istio is a service mesh: Envoy proxies intercept all service traffic to provide traffic management, mTLS, and observability with no app changes. **Two data-plane models — pick deliberately:**

- **Sidecar** (classic): one Envoy per pod; full L7 everywhere; higher per-pod CPU/mem and injection restart cost.
- **Ambient** (GA in 1.24): per-node **ztunnel** does L4 mTLS + L4 auth/telemetry automatically; **waypoint** proxies add L7 only where deployed. Lower overhead, no injection restarts, but L7 features silently no-op without a waypoint. See *Currency*.

### Quick Reference: Common Tasks

| Task                         | Resources                          | Section                               |
| ---------------------------- | ---------------------------------- | ------------------------------------- |
| Enable mTLS between services | PeerAuthentication                 | mTLS PeerAuthentication               |
| Route traffic to new version | VirtualService + DestinationRule   | Traffic Splitting for Canary          |
| Add circuit breaker          | DestinationRule (outlierDetection) | Circuit Breaker and Retry             |
| Configure retries/timeouts   | VirtualService (retries, timeout)  | Circuit Breaker and Retry             |
| Expose service to internet   | Gateway + VirtualService           | Gateway and VirtualService            |
| Control egress traffic       | Sidecar + ServiceEntry             | Sidecar Resource for Egress           |
| Add authorization rules      | AuthorizationPolicy                | AuthorizationPolicy for RBAC          |
| Configure load balancing     | DestinationRule (loadBalancer)     | DestinationRule with Traffic Policies |
| Test resilience              | VirtualService (fault injection)   | Fault Injection for Testing           |

### Architecture Components

#### Control Plane (istiod)

`istiod` is a single unified binary (since Istio 1.5) that performs:

- Service discovery and configuration distribution (xDS to the Envoy sidecars)
- Certificate authority for mTLS
- Sidecar injection (mutating admission webhook)
- Configuration validation

(Pilot, Galley, and Citadel were separate processes only through Istio 1.4; they were consolidated into `istiod` in 1.5.)

#### Data Plane

- Envoy proxies deployed as sidecars
- Intercept all inbound and outbound traffic
- Enforce policies and collect telemetry
- Handle traffic routing, load balancing, and retries

#### Key Resources

- `Gateway`: Configures load balancers for HTTP/TCP traffic entering the mesh
- `VirtualService`: Defines traffic routing rules
- `DestinationRule`: Configures policies after routing (load balancing, connection pools, circuit breakers)
- `ServiceEntry`: Adds external services to the mesh
- `PeerAuthentication`: Configures mTLS between services
- `AuthorizationPolicy`: Defines access control policies
- `Sidecar`: Controls sidecar proxy configuration and egress traffic

## Installation and Configuration

### Install Istio with Helm (recommended)

Helm is the officially recommended primary install method. The in-cluster
`IstioOperator` controller was deprecated in 1.23 and **removed in 1.24** — do
not apply a live `IstioOperator` CR and expect a controller to reconcile it.

```bash
helm repo add istio https://istio-release.storage.googleapis.com/charts
helm repo update

# Base CRDs + control plane
helm install istio-base istio/base -n istio-system --create-namespace
helm install istiod istio/istiod -n istio-system --wait

# Ingress gateway (optional, separate chart)
helm install istio-ingress istio/gateway -n istio-ingress --create-namespace

# Enable automatic sidecar injection for a namespace
kubectl label namespace default istio-injection=enabled
```

### Install Istio with istioctl

`istioctl install -f <file>` is also valid. Note that the `IstioOperator` kind
passed via `-f` is used here only as a **config-file format**, not a live CR
reconciled in-cluster.

```bash
# Download and install istioctl
curl -L https://istio.io/downloadIstio | sh -
cd istio-*
export PATH=$PWD/bin:$PATH

# Install with a profile (IstioOperator config-file, not a live CR)
istioctl install --set profile=default -y

# Verify installation
kubectl get pods -n istio-system
istioctl verify-install
istioctl analyze --all-namespaces
```

### Configuration Profiles

```bash
# Minimal: Control plane only, no ingress/egress
istioctl install --set profile=minimal

# Default: Recommended for production
istioctl install --set profile=default

# Demo: all features on, NOT for production (high resource use)
istioctl install --set profile=demo

# Custom configuration (tracing is now configured via the Telemetry API
# with an OpenTelemetry extensionProvider — see Expert Practices below —
# not the legacy meshConfig.enableTracing boolean)
istioctl install --set profile=default \
  --set meshConfig.accessLogFile=/dev/stdout \
  --set meshConfig.defaultConfig.proxyMetadata.ISTIO_META_DNS_CAPTURE=true
```

### Verify Sidecar Injection

```bash
# Check if namespace has injection enabled
kubectl get namespace -L istio-injection

# Verify pod has sidecar
kubectl get pod <pod-name> -o jsonpath='{.spec.containers[*].name}'
# Should show: app-container istio-proxy

# View sidecar configuration
istioctl proxy-config all <pod-name>.<namespace>
```

## Best Practices

Non-obvious defaults and decision points only; the failure modes behind these live in *Expert Practices*.

- **Gateway:** dedicated Gateway per domain/protocol in `istio-system`/`istio-ingress`; never bind two Gateways to the same port+host (last-writer-wins, non-deterministic); TLS secret must live in the gateway's namespace.
- **Routing:** exactly one VirtualService should own a given host — a second VS on the same host does **not** merge, it clobbers routes non-deterministically (see *Expert Practices → catch-all VS*). Subsets are declared in the **DestinationRule** and referenced in the **VirtualService** — the classic mismatch bug.
- **Resilience:** circuit-break via `outlierDetection`; cap `connectionPool`; keep `overall_timeout > attempts × perTryTimeout`; never blindly retry non-idempotent paths.
- **mTLS:** STRICT in prod, PERMISSIVE only mid-migration, scoped per namespace/workload. Verify effective mode with `istioctl proxy-config cluster <pod>.<ns> --fqdn <svc> -o json` and `istioctl x describe pod <pod>.<ns>` (`istioctl authn tls-check` was removed in 1.9).
- **AuthZ:** deny-all baseline first, then layer ALLOW (an empty-spec `ALLOW` policy denies all — Istio's default is *allow* when no policy selects the workload). DENY beats ALLOW.
- **Certs:** workload certs auto-rotate (~24h SDS certs; the intermediate CA cert defaults to 1 year); use an external root CA (cert-manager/Vault) in prod.
- **Observability:** monitor golden signals; **sample traces** (≪100% in prod); enable access logs selectively (perf cost). Trace context must be propagated by the app (see *Expert Practices → tracing*).

## Production-Ready Examples

### Gateway and VirtualService

```yaml
# gateway.yaml
apiVersion: networking.istio.io/v1
kind: Gateway
metadata:
  name: public-gateway
  namespace: istio-system
spec:
  selector:
    istio: ingressgateway
  servers:
    # HTTPS configuration
    - port:
        number: 443
        name: https
        protocol: HTTPS
      tls:
        mode: SIMPLE
        credentialName: tls-cert-secret # Secret in istio-system namespace
      hosts:
        - "api.example.com"
        - "app.example.com"
    # HTTP to HTTPS redirect
    - port:
        number: 80
        name: http
        protocol: HTTP
      hosts:
        - "api.example.com"
        - "app.example.com"
      tls:
        httpsRedirect: true
---
# virtualservice.yaml
apiVersion: networking.istio.io/v1
kind: VirtualService
metadata:
  name: api-routes
  namespace: default
spec:
  hosts:
    - "api.example.com"
  gateways:
    - istio-system/public-gateway
  http:
    # Route /v2 to new service
    - match:
        - uri:
            prefix: "/v2/"
      rewrite:
        uri: "/"
      route:
        - destination:
            host: api-v2.default.svc.cluster.local
            port:
              number: 8080
      timeout: 30s
      retries:
        attempts: 3
        perTryTimeout: 10s
        retryOn: 5xx,reset,connect-failure,refused-stream
    # Route /v1 to legacy service
    - match:
        - uri:
            prefix: "/v1/"
      route:
        - destination:
            host: api-v1.default.svc.cluster.local
            port:
              number: 8080
      timeout: 60s
    # Default route
    - route:
        - destination:
            host: api-v2.default.svc.cluster.local
            port:
              number: 8080
```

### DestinationRule with Traffic Policies

```yaml
apiVersion: networking.istio.io/v1
kind: DestinationRule
metadata:
  name: api-destination
  namespace: default
spec:
  host: api.default.svc.cluster.local
  trafficPolicy:
    # Load balancing
    loadBalancer:
      consistentHash:
        httpHeaderName: x-user-id # Session affinity
    # Connection pool settings
    connectionPool:
      tcp:
        maxConnections: 100
        connectTimeout: 30ms
        tcpKeepalive:
          time: 7200s
          interval: 75s
      http:
        http1MaxPendingRequests: 50
        http2MaxRequests: 100
        maxRequestsPerConnection: 2
        maxRetries: 3
    # Outlier detection (circuit breaker)
    outlierDetection:
      consecutive5xxErrors: 5
      interval: 30s
      baseEjectionTime: 30s
      maxEjectionPercent: 50
      minHealthPercent: 40
    # NOTE: no tls block. For intra-mesh hosts, auto-mTLS (default since 1.5)
    # negotiates mTLS automatically — setting tls.mode: ISTIO_MUTUAL is redundant,
    # and setting DISABLE/SIMPLE while PeerAuthentication is STRICT causes 503s.
    # Set trafficPolicy.tls only for TLS origination to NON-mesh external services
    # (SIMPLE/MUTUAL with caCertificates + subjectAltNames). Inspect the effective
    # mode with: istioctl proxy-config cluster <pod> --fqdn <svc> -o json
  # Define subsets for version-based routing
  subsets:
    - name: v1
      labels:
        version: v1
      trafficPolicy:
        loadBalancer:
          simple: ROUND_ROBIN
    - name: v2
      labels:
        version: v2
      trafficPolicy:
        loadBalancer:
          simple: LEAST_REQUEST
```

### Traffic Splitting for Canary Deployment

```yaml
apiVersion: networking.istio.io/v1
kind: VirtualService
metadata:
  name: canary-rollout
  namespace: default
spec:
  hosts:
    - reviews.default.svc.cluster.local
  http:
    # Send 10% of traffic to canary
    - match:
        - headers:
            x-canary:
              exact: "true"
      route:
        - destination:
            host: reviews.default.svc.cluster.local
            subset: v2
    - route:
        - destination:
            host: reviews.default.svc.cluster.local
            subset: v1
          weight: 90
        - destination:
            host: reviews.default.svc.cluster.local
            subset: v2
          weight: 10
---
# Blue-Green Deployment (instant switch)
apiVersion: networking.istio.io/v1
kind: VirtualService
metadata:
  name: blue-green
  namespace: default
spec:
  hosts:
    - orders.default.svc.cluster.local
  http:
    - route:
        # Switch to green by changing weight to 100
        - destination:
            host: orders.default.svc.cluster.local
            subset: blue
          weight: 100
        - destination:
            host: orders.default.svc.cluster.local
            subset: green
          weight: 0
```

### Circuit Breaker and Retry Configuration

```yaml
apiVersion: networking.istio.io/v1
kind: DestinationRule
metadata:
  name: circuit-breaker
  namespace: default
spec:
  host: backend.default.svc.cluster.local
  trafficPolicy:
    connectionPool:
      tcp:
        maxConnections: 10
      http:
        http1MaxPendingRequests: 1
        http2MaxRequests: 10
        maxRequestsPerConnection: 1
    outlierDetection:
      # Remove instance after 5 consecutive errors
      consecutive5xxErrors: 5
      consecutiveGatewayErrors: 5
      # Check every 1 second
      interval: 1s
      # Keep instance ejected for 30 seconds
      baseEjectionTime: 30s
      # Eject at most half the pool so the remainder keeps serving traffic
      maxEjectionPercent: 50
      # Stop ejecting once fewer than 40% of endpoints remain healthy.
      # minHealthPercent prevents a full blackout: maxEjectionPercent: 100 /
      # minHealthPercent: 0 ejects every endpoint -> all requests 503. Use 100/0
      # only in isolated test environments, never in production.
      minHealthPercent: 40
---
apiVersion: networking.istio.io/v1
kind: VirtualService
metadata:
  name: retry-policy
  namespace: default
spec:
  hosts:
    - payment.default.svc.cluster.local
  http:
    - route:
        - destination:
            host: payment.default.svc.cluster.local
      # Overall timeout must exceed attempts * perTryTimeout (3 * 3s = 9s),
      # else later attempts never complete before the overall timeout fires.
      timeout: 10s
      retries:
        attempts: 3
        perTryTimeout: 3s
        # Connection-level conditions only. retriable-4xx matches ONLY HTTP 409
        # (optimistic-lock conflicts that fail identically on retry) — omit it for
        # non-idempotent/financial paths like payments to avoid retry storms.
        retryOn: connect-failure,refused-stream,unavailable,reset
```

### mTLS PeerAuthentication

```yaml
# Namespace-wide STRICT mTLS
apiVersion: security.istio.io/v1
kind: PeerAuthentication
metadata:
  name: default-mtls
  namespace: production
spec:
  mtls:
    mode: STRICT
---
# Mesh-wide mTLS (apply to istio-system)
apiVersion: security.istio.io/v1
kind: PeerAuthentication
metadata:
  name: mesh-mtls
  namespace: istio-system
spec:
  mtls:
    mode: STRICT
---
# Workload-specific PERMISSIVE (migration)
apiVersion: security.istio.io/v1
kind: PeerAuthentication
metadata:
  name: legacy-service
  namespace: default
spec:
  selector:
    matchLabels:
      app: legacy-app
  mtls:
    mode: PERMISSIVE # Accept both mTLS and plaintext
  # Port-level override
  portLevelMtls:
    8080:
      mode: DISABLE # Health check port
---
# Inspect the effective TLS mode (transport socket) for a pod->service pair:
# istioctl proxy-config cluster <pod>.<namespace> --fqdn <service-fqdn> -o json
# Per-pod summary:
# istioctl x describe pod <pod>.<namespace>
```

### AuthorizationPolicy for RBAC

```yaml
# Default deny all
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: deny-all
  namespace: production
spec: {} # Empty spec denies all requests
---
# Allow specific service-to-service communication
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: allow-frontend-to-backend
  namespace: production
spec:
  selector:
    matchLabels:
      app: backend
  action: ALLOW
  rules:
    # Allow from frontend service
    - from:
        - source:
            principals:
              - "cluster.local/ns/production/sa/frontend"
      to:
        - operation:
            methods: ["GET", "POST"]
            paths: ["/api/*"]
---
# JWT authentication and authorization
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: require-jwt
  namespace: default
spec:
  selector:
    matchLabels:
      app: api
  action: ALLOW
  rules:
    - from:
        - source:
            requestPrincipals: ["*"] # Valid JWT required
      when:
        - key: request.auth.claims[role]
          values: ["admin", "user"]
---
# IP-based allow list
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: allow-internal-ips
  namespace: default
spec:
  selector:
    matchLabels:
      app: admin-panel
  action: ALLOW
  rules:
    - from:
        - source:
            ipBlocks: ["10.0.0.0/8", "172.16.0.0/12"]
---
# Method and path-based restrictions
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: read-only-access
  namespace: default
spec:
  selector:
    matchLabels:
      app: database-api
  action: ALLOW
  rules:
    - to:
        - operation:
            methods: ["GET", "HEAD"]
  # DENY takes precedence over ALLOW
---
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: deny-delete
  namespace: default
spec:
  selector:
    matchLabels:
      app: database-api
  action: DENY
  rules:
    - to:
        - operation:
            methods: ["DELETE"]
```

### Sidecar Resource for Egress Control

```yaml
# Default sidecar for namespace (restrict egress)
apiVersion: networking.istio.io/v1
kind: Sidecar
metadata:
  name: default-sidecar
  namespace: production
spec:
  # Apply to all workloads in namespace
  egress:
    # Allow access to services in same namespace
    - hosts:
        - "./*"
    # Allow access to istio-system
    - hosts:
        - "istio-system/*"
    # Allow specific external services
    - hosts:
        - "*/external-api.external.svc.cluster.local"
---
# Workload-specific sidecar
apiVersion: networking.istio.io/v1
kind: Sidecar
metadata:
  name: frontend-sidecar
  namespace: default
spec:
  workloadSelector:
    labels:
      app: frontend
  ingress:
    - port:
        number: 8080
        protocol: HTTP
        name: http
      defaultEndpoint: 127.0.0.1:8080
  egress:
    # Only allow access to backend service
    - hosts:
        - "./backend.default.svc.cluster.local"
    # Allow access to external API
    - hosts:
        - "*/api.external.com"
---
# Optimize sidecar for external service access
apiVersion: networking.istio.io/v1
kind: ServiceEntry
metadata:
  name: external-api
  namespace: default
spec:
  hosts:
    - api.external.com
  ports:
    - number: 443
      name: https
      protocol: HTTPS
  location: MESH_EXTERNAL
  resolution: DNS
---
apiVersion: networking.istio.io/v1
kind: Sidecar
metadata:
  name: external-egress
  namespace: default
spec:
  workloadSelector:
    labels:
      app: worker
  outboundTrafficPolicy:
    mode: REGISTRY_ONLY # Only allow registered ServiceEntry
  egress:
    - hosts:
        - "*/api.external.com"
```

## Advanced Patterns

### Fault Injection for Testing

> ⚠ Fault injection is **silently ignored** on a route that also has retry/timeout
> (see *Expert Practices → Traffic Management*). Inject the fault on the upstream
> proxy via a separate `EnvoyFilter`.

```yaml
apiVersion: networking.istio.io/v1
kind: VirtualService
metadata:
  name: fault-injection
  namespace: default
spec:
  hosts:
    - ratings.default.svc.cluster.local
  http:
    - match:
        - headers:
            x-test:
              exact: "chaos"
      fault:
        # Inject 5 second delay for 50% of requests
        delay:
          percentage:
            value: 50.0
          fixedDelay: 5s
        # Abort 10% of requests with HTTP 500
        abort:
          percentage:
            value: 10.0
          httpStatus: 500
      route:
        - destination:
            host: ratings.default.svc.cluster.local
    - route:
        - destination:
            host: ratings.default.svc.cluster.local
```

### Multi-Cluster Service Mesh

Every cluster shares one `meshID`; each gets a unique `clusterName` and `network`.
Config-file input to `istioctl install -f` (or Helm `values`) — **not** a live CR
(`kubectl apply`), since the in-cluster operator was removed in 1.24.

```yaml
# istioctl install -f <cluster>.yaml — set clusterName/network per cluster
apiVersion: install.istio.io/v1alpha1
kind: IstioOperator
spec:
  values:
    global:
      meshID: mesh1
      multiCluster: { clusterName: primary } # or: remote
      network: network1 # unique per cluster
      # remote clusters also set: remotePilotAddress: <primary istiod address>
```

### Locality-Based Load Balancing

```yaml
apiVersion: networking.istio.io/v1
kind: DestinationRule
metadata:
  name: locality-lb
  namespace: default
spec:
  host: service.default.svc.cluster.local
  trafficPolicy:
    loadBalancer:
      localityLbSetting:
        enabled: true
        # Prefer same region/zone
        distribute:
          - from: us-west/zone1/*
            to:
              "us-west/zone1/*": 80
              "us-west/zone2/*": 20
        # Failover configuration
        failover:
          - from: us-west
            to: us-east
    # REQUIRED for locality failover to function: without outlierDetection,
    # proxies cannot mark endpoints unhealthy, so failover never triggers
    # (silently — no error). consecutiveErrors was renamed consecutive5xxErrors
    # in 1.9+; the old name is silently ignored and disables detection.
    outlierDetection:
      consecutive5xxErrors: 5
      interval: 1s
      baseEjectionTime: 1m
```

## Troubleshooting Commands

```bash
# Check Istio installation
istioctl verify-install

# Analyze configuration issues
istioctl analyze --all-namespaces

# Inspect proxy configuration
istioctl proxy-config cluster <pod-name>.<namespace>
istioctl proxy-config route <pod-name>.<namespace>
istioctl proxy-config listener <pod-name>.<namespace>
istioctl proxy-config endpoint <pod-name>.<namespace>

# Check effective mTLS / TLS mode (istioctl authn tls-check was removed in 1.9)
istioctl proxy-config cluster <pod-name>.<namespace> --fqdn <service-fqdn> -o json
istioctl x describe pod <pod-name>.<namespace>

# View proxy logs
kubectl logs <pod-name> -c istio-proxy -n <namespace>

# Debug routing
istioctl experimental describe pod <pod-name> -n <namespace>

# Check certificate expiration
istioctl proxy-config secret <pod-name>.<namespace> -o json | jq '.dynamicActiveSecrets[0].secret.tlsCertificate.certificateChain.inlineBytes' -r | base64 -d | openssl x509 -text -noout

# Test traffic routing
kubectl exec <pod-name> -c istio-proxy -- curl -v http://service:port/path

# Export proxy configuration for debugging
istioctl proxy-config all <pod-name>.<namespace> -o json > proxy-config.json
```

## Performance Tuning

### Resource Requests and Limits

```yaml
# Sidecar proxy resources
apiVersion: v1
kind: Namespace
metadata:
  name: production
  annotations:
    # Set default sidecar resources
    sidecar.istio.io/proxyCPU: "100m"
    sidecar.istio.io/proxyCPULimit: "2000m"
    sidecar.istio.io/proxyMemory: "128Mi"
    sidecar.istio.io/proxyMemoryLimit: "1024Mi"
```

### Control Plane Tuning

```yaml
apiVersion: install.istio.io/v1alpha1
kind: IstioOperator
spec:
  meshConfig:
    # Reduce config push time
    defaultConfig:
      holdApplicationUntilProxyStarts: true
      proxyMetadata:
        ISTIO_META_DNS_CAPTURE: "true"
        ISTIO_META_DNS_AUTO_ALLOCATE: "true"
  components:
    pilot:
      k8s:
        resources:
          requests:
            cpu: 500m
            memory: 2Gi
          limits:
            cpu: 2000m
            memory: 4Gi
```

> Several `PILOT_*` env vars changed meaning or were removed as istiod's config
> distribution was rearchitected across 1.20–1.26 — verify any tuning flag
> against your exact Istio version and the official
> [performance and scalability](https://istio.io/latest/docs/ops/deployment/performance-and-scalability/)
> docs before setting it. The highest-leverage scaling knob is usually
> **configuration scoping** via a namespace `Sidecar` resource (see Expert
> Practices below), not control-plane env vars.

## Security Hardening

### Remove NET_ADMIN/NET_RAW from app pods (Istio CNI)

`meshConfig.defaultConfig.runAsUser` / `securityContext` is **not** a valid path
in the IstioOperator/meshConfig API — it is silently ignored, giving false
confidence that hardening was applied. The supported hardening is the **Istio CNI
plugin**, which moves iptables setup out of an init container so application pods
no longer need the `NET_ADMIN`/`NET_RAW` capabilities. Proxy security context is
set via `values.global.proxy`, not `meshConfig.defaultConfig`.

```yaml
# istioctl install -f cni.yaml  (config-file, not a live CR)
apiVersion: install.istio.io/v1alpha1
kind: IstioOperator
spec:
  components:
    cni:
      enabled: true # no NET_ADMIN/NET_RAW needed on app pods
  values:
    global:
      proxy:
        privileged: false
```

> Confirm hardening actually took effect with `istioctl verify-install` and
> `istioctl analyze` — never assume a config block applied just because `apply`
> succeeded.

### Egress Traffic Control

```yaml
# Block all egress by default
apiVersion: install.istio.io/v1alpha1
kind: IstioOperator
spec:
  meshConfig:
    outboundTrafficPolicy:
      mode: REGISTRY_ONLY # Only allow registered ServiceEntry
```

## Migration Strategy

Adopt incrementally; **never jump straight to mesh-wide STRICT** — any un-injected client will break.

1. **Install, no injection:** `istioctl install --set profile=default`.
2. **Inject per workload:** label the namespace `istio-injection=enabled` (or `istio.io/rev=<rev>` for revision-based), then roll pods. Sidecars attach only on pod restart.
3. **PERMISSIVE mTLS** mesh-wide (`PeerAuthentication` in `istio-system`, `mtls.mode: PERMISSIVE`) — meshed and un-meshed clients coexist.
4. **Verify** every workload actually negotiates mTLS before tightening:

   ```bash
   for pod in $(kubectl get pods -n production -o jsonpath='{.items[*].metadata.name}'); do
     istioctl x describe pod "$pod.production"   # authn tls-check removed in 1.9
   done
   ```

5. **STRICT mTLS** once all traffic is verified mutual (flip the same `PeerAuthentication` to `mode: STRICT`).

## Expert Practices: Idioms, Anti-Patterns & Gotchas

Hard-won, documentation-backed rules. Most Istio outages on otherwise-valid YAML
trace to one of these.

### Authorization Policy

**Rule logic — AND within a rule, OR across rules.** Inside one rule element,
`from` / `to` / `when` are **AND**-ed (all must match). Separate rule-list
elements (each `-` bullet) are **OR**-ed (matching ANY rule passes). An accidental
extra `-` silently turns a compound requirement into a far more permissive OR.
Evaluation order across actions is **CUSTOM → DENY → ALLOW**; one matching DENY
overrides all ALLOW. If **no** ALLOW policy selects a workload, traffic is allowed
by default — so apply a deny-all baseline first, then layer ALLOW rules.

```yaml
# Single rule = AND: principal AND method/path AND claim all required
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
spec:
  action: ALLOW
  rules:
    - from:
        - source: { principals: ["cluster.local/ns/frontend/sa/frontend"] }
      to:
        - operation: { methods: ["GET"], paths: ["/api/*"] }
      when:
        - key: request.auth.claims[role]
          values: ["admin"]
```

> **`spec: {}` vs `rules: - {}` are opposites.** An empty `spec: {}` matches **no**
> rule and therefore **denies everything** (the correct deny-all baseline). An
> empty rule `rules: - {}` matches **every** request and **allows everything** — a
> two-character YAML difference with inverse meaning.

**Prefer ALLOW + positive matching over DENY + `notPaths`.** Policy is enforced on
the path as Envoy normalizes it (default mode **BASE**: RFC 3986 + backslash→slash,
but does **not** merge `//` or decode `%2F`), then a possibly-different path is
forwarded to the backend. A `DENY` on `/admin/secret` can be bypassed by
`/admin//secret` or `/admin%2Fsecret` if the backend collapses/decodes differently.
Mitigate by (1) setting `meshConfig.pathNormalization.normalization` (e.g.
`MERGE_SLASHES` or `DECODE_AND_MERGE_SLASHES` — mesh-wide only, test routing first)
to match your backend, and (2) using ALLOW with `paths: [...]`, whose worst-case
mismatch is a safe 403, never a bypass. `notPaths` belongs with DENY, not ALLOW.

> **DENY + HTTP-only fields on a TCP port denies ALL traffic.** In a DENY rule an
> unresolvable attribute (`methods`/`paths`/`hosts` on an unnamed/DB port treated
> as TCP) is treated as a **match** — so a DENY scoped by `methods: [DELETE]` on a
> TCP port denies everything on that port. (ALLOW is the inverse: the field never
> matches, allowing everything.) Always scope DENY policies to an explicitly
> HTTP-named port: `to: [{ operation: { ports: ["8080"], methods: ["DELETE"] } }]`.
>
> **`operation.hosts` is effective on gateways only, not workload sidecars.** A
> server-side sidecar ignores the Host header when redirecting to the app, so a
> `hosts:` rule on a workload selector is bypassable by hitting the pod IP directly
> with any Host header. Put host-based access control on the ingress gateway.
>
> **PERMISSIVE mTLS makes `source.principals` unreliable.** Plaintext requests
> carry an empty SPIFFE principal; any ALLOW rule that does **not** check the
> principal (e.g. method/path only) still admits unauthenticated plaintext, giving
> false confidence. Only **STRICT** PeerAuthentication guarantees a populated
> `source.principals` reflects a verified mTLS identity — enforce STRICT before
> relying on principal-based authorization.

### Traffic Management

**Make-before-break: subset config order matters.** Istio distributes config with
eventual consistency — there is no atomic cross-resource apply. If a VirtualService
referencing subset `v2` reaches a sidecar before the DestinationRule defining `v2`,
every routed request 503s (NR/No Route) until both propagate. **Adding** a subset:
apply the DestinationRule first, wait for propagation, then the VirtualService.
**Removing** a subset: update the VirtualService to stop routing to it first, wait,
then remove the subset. This is the leading cause of transient 503s during
canary/blue-green rollouts.

```bash
kubectl apply -f destination-rule-with-v2-subset.yaml
sleep 5   # allow xDS propagation
kubectl apply -f virtual-service-routing-to-v2.yaml
```

**Two VirtualServices on the same host clobber each other.** Istio does **not**
merge multiple VS for one host+gateway — behavior is undefined and effectively
last-writer-wins, and a broad catch-all VS (e.g. `hosts: ["*"]` or a bare service
host with only a default route) silently swallows the routes of a more specific VS.
`istioctl analyze` flags this as `conflicting`/`overlapping` but does not block apply.
One VS per host; express all routes (canary, header match, default) as ordered `http`
rules **within that single VS** — order matters, first match wins, so put specific
matches before the default.

**Place a DestinationRule in the SAME namespace as its Service.** Istio resolves a
DestinationRule by searching exactly three namespaces in order: the **client's**
ns, the **service's** ns, then the **root** ns (`meshConfig.rootNamespace`, default
`istio-system`). `exportTo` controls visibility but is orthogonal — a rule exported
everywhere from `ns1` still won't apply to a client in `ns2` calling a service in
`ns3`. A shared "config" namespace that is neither client, service, nor root is
silently ignored.

**Fault injection and retry/timeout cannot coexist on one route.** When both are on
the same VirtualService HTTP route, the retry/timeout is silently dropped (Envoy
fault filter runs before the router filter). Keep retries on the client
VirtualService; inject the fault on the upstream proxy via a separate `EnvoyFilter`.

**Size the overall timeout above `attempts × perTryTimeout`.** With `timeout: 10s`,
`attempts: 3`, `perTryTimeout: 5s`, the 10s overall timeout kills the request before
the third attempt's 5s window — design so `overall_timeout > attempts × perTryTimeout`.
Avoid `retriable-4xx` (matches **only** HTTP 409, often an optimistic-lock conflict
that fails identically on retry) for state-mutating/financial ops; prefer
`connect-failure,refused-stream,unavailable,reset`.

**Locality LB / failover does nothing without `outlierDetection`.** Locality-priority
and failover both **require** an `outlierDetection` block — without it proxies can't
mark endpoints unhealthy, so spillover never triggers (no error). Configure at
minimum `consecutive5xxErrors`, `interval`, `baseEjectionTime`.

**Declare protocol explicitly — auto-detection failure falls back silently to TCP.**
When HTTP detection is ambiguous, traffic is treated as plain TCP and **every** L7
feature (retries, timeouts, header/path routing, AuthorizationPolicy method/path
checks, HTTP telemetry) silently stops — traffic still flows. Name the port
(`name: http-...`) or set `appProtocol: http` (Kubernetes 1.18+; `appProtocol` wins
when both are set). Server-first protocols (MySQL, MongoDB, SMTP) are incompatible
with byte-sniffing AND PERMISSIVE-mTLS sniffing — name those ports and disable mTLS
per-port (`portLevelMtls: { 3306: { mode: DISABLE } }`).

### Security

**Don't set `trafficPolicy.tls` for intra-mesh traffic.** Auto-mTLS (default since
1.5) handles encryption between meshed workloads with no DestinationRule TLS config.
`ISTIO_MUTUAL` for an intra-mesh host is redundant; `DISABLE`/`SIMPLE` against a host
whose server enforces STRICT PeerAuthentication sends plaintext and gets an immediate
503. Reserve `trafficPolicy.tls` (SIMPLE/MUTUAL) for TLS origination to non-mesh
external services.

**TLS origination needs BOTH `caCertificates` AND `subjectAltNames`.** `caCertificates`
alone proves the upstream cert was signed by a trusted CA but **not** that it was
issued to the intended host — so any cert from the same CA (e.g. an attacker's) is
accepted, enabling MITM. Always set both (plus `sni`).

```yaml
apiVersion: networking.istio.io/v1
kind: DestinationRule
spec:
  host: external-api.example.com
  trafficPolicy:
    tls:
      mode: SIMPLE
      caCertificates: /etc/ssl/certs/ca-certificates.crt
      subjectAltNames: ["external-api.example.com"] # required for host verification
      sni: external-api.example.com
```

**`outboundTrafficPolicy: REGISTRY_ONLY` is not a security boundary.** It restricts
only the sidecar's routing table; because app and sidecar share a network namespace,
a compromised/misconfigured workload can bypass its **own** sidecar (iptables, root)
and reach arbitrary IPs. Enforce egress with a kernel-level Kubernetes
`NetworkPolicy` AND route through a dedicated Egress Gateway. (Note the asymmetry: a
workload cannot bypass the **server-side** sidecar, so server-side
AuthorizationPolicy + STRICT mTLS remain meaningful.)

**App containers must not run as UID 1337.** That UID is reserved for istio-proxy;
the iptables rules exempt UID 1337 to avoid an interception loop. An app running as
1337 has its outbound traffic silently bypass the sidecar — no mTLS, no
AuthorizationPolicy, no telemetry. Use any other UID (`runAsUser: 1000`,
`runAsNonRoot: true`).

### Performance

**Scope per-proxy config with a namespace `Sidecar` resource at scale.** By default
istiod distributes config for the **whole mesh** to **every** Envoy, so each proxy's
xDS memory scales with total mesh size, not its dependencies. A namespace-wide
`Sidecar` with a restricted `egress.hosts` shrinks each proxy's xDS payload and
istiod's per-proxy diff CPU. Only one namespace-wide Sidecar (no `workloadSelector`)
is allowed per namespace.

```yaml
apiVersion: networking.istio.io/v1
kind: Sidecar
metadata: { name: default, namespace: payments }
spec:
  egress:
    - hosts:
        - "./*" # same namespace
        - "istio-system/*" # control plane
        - "shared/db-service" # explicit cross-ns dependency
```

### Observability

**Distributed tracing is silently broken without app-level header propagation.**
Sidecars auto-generate a span per hop but cannot link a service's inbound request to
its outbound calls — the **application** must copy trace-context headers from each
incoming request onto every outgoing request. Forward `x-request-id`, W3C
`traceparent`/`tracestate`, and B3 `x-b3-traceid`/`x-b3-spanid`/`x-b3-parentspanid`/
`x-b3-sampled`/`x-b3-flags`. This is application code in every service, not mesh
config.

**Configure tracing via the Telemetry API + OpenTelemetry provider.** OpenCensus
tracing was removed in 1.25 and Lightstep deprecated in 1.22; the legacy
`meshConfig.tracing`/`enableTracing` path is obsolete. Define an OTel collector as a
`meshConfig.extensionProviders` entry and enable tracing (with sampling) via a
`Telemetry` resource.

```yaml
# 1. Register the OTel collector as an extension provider (install-time config)
apiVersion: install.istio.io/v1alpha1
kind: IstioOperator
spec:
  meshConfig:
    extensionProviders:
      - name: otel-tracing
        opentelemetry:
          port: 4317
          service: opentelemetry-collector.observability.svc.cluster.local
---
# 2. Enable tracing against that provider via the Telemetry API
apiVersion: telemetry.istio.io/v1
kind: Telemetry
metadata: { name: mesh-tracing, namespace: istio-system }
spec:
  tracing:
    - providers: [{ name: otel-tracing }]
      randomSamplingPercentage: 1.0
```

### Currency (Istio 1.22–1.25)

- **Use `v1` API versions.** 1.22 promoted DestinationRule, Gateway, ServiceEntry,
  Sidecar, VirtualService, WorkloadEntry, WorkloadGroup, PeerAuthentication, and
  Telemetry to `v1` (AuthorizationPolicy has been `v1` since 1.9). Prefer
  `networking.istio.io/v1` and `security.istio.io/v1`; the optional stable
  validation policy (Kubernetes 1.30+) can enforce v1-only at admission.
- **The in-cluster IstioOperator controller was removed in 1.24.** Install via Helm
  (recommended) or `istioctl install`; an `IstioOperator` CR is only valid as a
  config file passed to `istioctl install -f`, not a live reconciled resource.
- **Canary control-plane upgrades use revision labels, not `istio-injection`.** Install
  the new control plane with `istioctl install --revision=1-24-0` (or
  `revision=...` in Helm), then migrate namespaces from `istio-injection=enabled` to
  `istio.io/rev=1-24-0` and roll pods — proxies re-attach to the new revision on
  restart, so you can canary and roll back per namespace. A namespace carrying **both**
  `istio-injection` and `istio.io/rev` is ambiguous: `istio-injection` wins and the
  revision label is ignored, silently pinning pods to the default revision. Tag a
  revision as default (`istioctl tag set default --revision ...`) so `istio-injection`
  namespaces follow the intended control plane.
- **Ambient mode is GA in 1.24.** Two layers: a per-node **ztunnel** (mTLS, L4
  authorization, L4 telemetry — automatic for enrolled pods) and optional
  **waypoint** proxies for L7. ztunnel does **not** parse HTTP, so VirtualService/
  HTTPRoute routing, L7 AuthorizationPolicy (method/path/JWT-claim), retries/
  traffic-splitting, and HTTP telemetry require a waypoint — without one, L7 rules
  silently have no effect. Deploy a waypoint via the Gateway API
  (`gatewayClassName: istio-waypoint` or `istioctl waypoint apply`) and enroll with
  the `istio.io/use-waypoint` label. Ingress-originated traffic does not use the
  destination waypoint unless the Service has `istio.io/ingress-use-waypoint=true`.

```bash
istioctl waypoint apply -n production
kubectl label namespace production istio.io/use-waypoint=waypoint
```

- **Prefer the Kubernetes Gateway API for new deployments.** Gateway API mesh support
  is Stable as of 1.22. Unlike Istio's `Gateway` (which only configures an existing
  deployment), the Kubernetes `Gateway` resource both configures **and** deploys the
  gateway, and is the **required** mechanism for ambient waypoints. VirtualService/
  Gateway remain fully supported for sidecar mode and are not being removed, but
  HTTPRoute + Gateway are the strategic, portable successors.

```yaml
apiVersion: gateway.networking.k8s.io/v1
kind: Gateway
metadata: { name: public-gateway, namespace: istio-system }
spec:
  gatewayClassName: istio
  listeners:
    - name: https
      port: 443
      protocol: HTTPS
      tls:
        mode: Terminate
        certificateRefs: [{ name: tls-cert-secret }]
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata: { name: api-routes }
spec:
  parentRefs: [{ name: public-gateway, namespace: istio-system }]
  hostnames: ["api.example.com"]
  rules:
    - matches: [{ path: { type: PathPrefix, value: /v2/ } }]
      backendRefs: [{ name: api-v2, port: 8080 }]
```

## Verification Checklist

Before declaring an Istio change done:

- [ ] `istioctl analyze -n <ns>` (or `--all-namespaces`) is clean — no conflicting VS/DR, missing subsets, or unresolved refs
- [ ] Every DestinationRule subset referenced by a VirtualService exists **and** the DR was applied before the VS (make-before-break)
- [ ] Exactly one VirtualService owns each host; ordered `http` rules put specific matches before the default
- [ ] Ports are protocol-named (`name: http-…` or `appProtocol`) so L7 features engage — confirm with `istioctl proxy-config listener`
- [ ] mTLS mode is what you intend: `istioctl proxy-config cluster <pod> --fqdn <svc> -o json` shows the expected transport socket; STRICT enforced before relying on `source.principals`
- [ ] AuthorizationPolicy: deny-all baseline present; DENY rules scoped to HTTP-named ports; rule list uses AND-within/OR-across as intended (no stray `-`)
- [ ] `overall timeout > attempts × perTryTimeout`; `retryOn` excludes non-idempotent paths; `outlierDetection` present wherever failover/circuit-breaking is expected
- [ ] Ambient only: a waypoint is deployed and the workload/service is labeled `istio.io/use-waypoint` for any L7 rule to take effect
- [ ] No app container runs as UID 1337; sidecar actually injected (`kubectl get pod -o jsonpath='{.spec.containers[*].name}'` shows `istio-proxy`)

## Additional Resources

- [Istio Documentation](https://istio.io/latest/docs/)
- [Istio Best Practices](https://istio.io/latest/docs/ops/best-practices/)
- [Envoy Proxy Documentation](https://www.envoyproxy.io/docs)
- [Service Mesh Patterns](https://www.oreilly.com/library/view/istio-up-and/9781492043775/)
