---
name: loom-docker
description: Docker and container expertise. Use for writing Dockerfiles, docker-compose files, multi-stage builds, layer optimization, image security hardening, networking, volumes, registries, and container debugging.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - docker
  - container
  - dockerfile
  - image
  - compose
  - registry
  - build
  - layer
  - cache
  - multi-stage
  - volume
  - network
  - port
  - environment
  - containerize
  - orchestration
  - push
  - pull
  - tag
  - alpine
  - slim
  - distroless
---

# Docker

## Overview

Production-grade containers: Dockerfiles, compose, multi-stage builds, layer/cache optimization, security hardening, registries, debugging. The **Expert Practices** section below is the highest-value part — it states the *mechanism* behind each rule, which is what separates correct-but-naive Dockerfiles from production ones. Read it before writing anything non-trivial.

## Writing Dockerfiles

**Base image:** official images; `-alpine`/`-slim`/distroless/`scratch` for runtime; pin by tag AND digest (`python:3.12-slim@sha256:...`, see Currency), never `latest`; multi-stage to drop build toolchains.

**Layer ordering (cache):** least-changing first (base → system deps → dependency manifests → source). **Copy dependency manifests and install BEFORE copying source** so a code edit doesn't bust the dependency layer — this single ordering rule is the biggest cache win.

**Layer hygiene:**

- Keep `apt-get update` and `apt-get install` in the SAME `RUN` — in separate layers Docker reuses a stale cached `update` index when only the install list changes, silently installing outdated/vulnerable packages (docs call this "cache busting"). Details in Anti-Patterns.
- `--no-install-recommends`, clean caches in the same layer (`rm -rf /var/lib/apt/lists/*`).
- `.dockerignore` to keep context small (excludes `.git`, `node_modules`, build artifacts) — bloated context slows builds and busts cache.

**Runtime & security:** non-root numeric `USER uid:gid` (see Gotchas); `--read-only` root fs where possible; secrets via `RUN --mount=type=secret`, never `ENV`/`ARG`/`COPY` (see below); `HEALTHCHECK`; exec-form `CMD`/`ENTRYPOINT` (shell form breaks signals — see Anti-Patterns).

> **Why `ARG`/`ENV` are unsafe for secrets:** `ARG` (incl. `--build-arg`) and `ENV` are recorded in image metadata, visible via `docker history --no-trunc`, `docker inspect`, and provenance attestations to anyone who can pull; `ENV` is additionally readable at runtime via `/proc/self/environ`. Unsetting in a later `RUN` does NOT scrub it — already committed in the earlier layer. Use `RUN --mount=type=secret`. `ARG`/`ENV` are safe only for non-sensitive values (versions, build flags).

## Docker Compose

- **Deps:** `depends_on` with `condition:` (`service_healthy`, `service_completed_successfully`) — bare `depends_on` only orders start, doesn't wait for readiness.
- **Health:** `healthcheck` on every service other services wait on.
- **Restart:** `unless-stopped` / `on-failure` for production.
- **Networking:** named networks for isolation; databases/caches get no host port mapping; reference services by name (DNS); listen on `0.0.0.0` not `127.0.0.1`.
- **Volumes:** named volumes for data; bind mounts for dev code only.
- **Config:** `${VAR:-default}` interpolation; `secrets:` (file-backed) with `*_FILE` env convention, not plaintext env.
- **Limits:** `deploy.resources.limits` for prod.

## Security Hardening

- Minimal base (distroless/alpine) shrinks attack surface.
- Drop all Linux capabilities, add back only what's needed; pair with `no-new-privileges` (see Security).
- Non-root, read-only root, no `--privileged`.
- Scan in CI (trivy, grype, Docker Scout); keep bases current; generate SBOM + provenance (see Security).
- Secrets: never in Dockerfile/`ENV`/`ARG`; build-time `--mount=type=secret`; runtime via orchestrator secrets/vault.

## Debugging

```bash
docker logs -f --tail 100 <container>          # or: docker-compose logs -f app
docker exec -it <container> /bin/sh            # shell into running container
docker run --rm -it --entrypoint /bin/sh img   # shell into a broken image
docker build --progress=plain --no-cache .     # full build output
docker diff <container>                         # filesystem changes vs image
```

Tools: **dive** (layer bloat), **hadolint** (Dockerfile lint), **trivy**/**grype**/**Docker Scout** (CVEs), **ctop** (live resource monitor). For minimal/distroless images that lack a shell, debug via `docker run --entrypoint`, ephemeral debug containers (`kubectl debug`), or `docker cp` files out.

Common causes: permission-denied → volume ownership vs container UID; "name not found" → wrong network / used `localhost` instead of service name; missing shared libs → glibc-vs-musl (see Gotchas); container exits immediately → check `docker logs`, verify CMD/ENTRYPOINT executable and required env present.

## Language Notes

| Lang | Key points |
| --- | --- |
| Python | `python:3.x-slim` for glibc; `pip install --no-cache-dir`; wheel-cache in builder; `uv` for speed. |
| Node | Copy `package*.json` first; `npm ci` (reproducible) not `install`; ship prod deps via `npm ci --omit=dev` or `npm prune --omit=dev` AFTER build (bundler/TS are devDeps — see Currency); `node:alpine` for size. |
| Rust | `cargo chef` for dep caching; `--release` + `strip`; copy only the binary; `scratch`/distroless runtime; target musl for alpine/scratch. |
| Go | `golang:alpine` builder; `CGO_ENABLED=0` static binary; `scratch`/distroless runtime; copy only the binary + CA certs. |

## Examples

### Multi-stage Python (glibc, non-root, healthcheck)

```dockerfile
FROM python:3.12-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
COPY requirements.txt .
RUN pip wheel --no-cache-dir --no-deps --wheel-dir /app/wheels -r requirements.txt

FROM python:3.12-slim AS runtime
RUN groupadd --gid 1000 app && useradd --uid 1000 --gid app --create-home app
WORKDIR /app
COPY --from=builder /app/wheels /wheels
RUN pip install --no-cache-dir /wheels/* && rm -rf /wheels
COPY --chown=1000:1000 . .
USER 1000:1000            # numeric UID:GID for Kubernetes runAsNonRoot
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1
CMD ["gunicorn", "--bind", "0.0.0.0:8000", "--workers", "4", "app:create_app()"]
```

### Node with BuildKit cache mounts + prune

```dockerfile
# syntax=docker/dockerfile:1
FROM node:20-alpine AS base
WORKDIR /app

FROM base AS build
COPY package*.json ./
RUN --mount=type=cache,target=/root/.npm npm ci      # ALL deps: build needs devDeps
COPY . .
RUN npm run build && npm prune --omit=dev            # strip devDeps after build

FROM base AS production
RUN apk update && apk upgrade && rm -rf /var/cache/apk/* \
    && addgroup -g 1001 -S nodejs && adduser -S app -u 1001
COPY --from=build --chown=1001:1001 /app/dist ./dist
COPY --from=build --chown=1001:1001 /app/node_modules ./node_modules
COPY --from=build --chown=1001:1001 /app/package.json ./
USER 1001:1001
EXPOSE 3000
ENV NODE_ENV=production
CMD ["node", "dist/server.js"]
```

### Rust (cargo chef → distroless)

```dockerfile
FROM rust:1.75-alpine AS chef
RUN apk add --no-cache musl-dev && cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json   # cached dependency layer
COPY . .
RUN cargo build --release && strip target/release/myapp

FROM gcr.io/distroless/cc-debian12
COPY --link --from=builder /app/target/release/myapp /usr/local/bin/myapp
USER 65532:65532          # distroless "nonroot"; numeric so runAsNonRoot verifies it
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/myapp"]
```

### Go (static binary → scratch)

```dockerfile
FROM golang:1.22-alpine AS builder
WORKDIR /app
RUN apk add --no-cache ca-certificates tzdata
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -ldflags='-w -s' -o /app/bin/server ./cmd/server

FROM scratch
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/bin/server /server
USER 65534:65534          # scratch has no /etc/passwd — MUST be numeric
ENTRYPOINT ["/server"]
```

### Compose with secrets, healthcheck, limits

```yaml
services:
  app:
    build: { context: ., args: { APP_VERSION: "${APP_VERSION:-latest}" } }
    ports: ["8080:8080"]
    environment:
      - DATABASE_URL_FILE=/run/secrets/db_url      # *_FILE convention, not plaintext env
    secrets: [db_url]
    depends_on:
      db: { condition: service_healthy }           # waits for readiness, not just start
    restart: unless-stopped
    deploy:
      resources:
        limits: { cpus: "1.0", memory: 512M }
  db:
    image: postgres:16-alpine
    volumes: [postgres_data:/var/lib/postgresql/data]
    environment:
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password
    secrets: [db_password]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U myapp"]
      interval: 10s
      timeout: 5s
      retries: 5

secrets:
  db_url:      { file: ./secrets/db_url.txt }
  db_password: { file: ./secrets/db_password.txt }
volumes:
  postgres_data:
```

### .dockerignore

```gitignore
.git
node_modules
__pycache__
*.pyc
.venv
dist
build
.env
.env.*
Dockerfile*
docker-compose*
*.log
.DS_Store
```

### Multi-arch build (buildx)

```dockerfile
FROM --platform=${BUILDPLATFORM} golang:1.22-alpine AS builder
ARG TARGETARCH TARGETOS          # provided by buildx per target
WORKDIR /app
COPY . .
RUN CGO_ENABLED=0 GOOS=${TARGETOS} GOARCH=${TARGETARCH} go build -o /app/bin/server ./cmd/server
FROM alpine:3.21                 # pin runtime base (never latest); static binary could use scratch
COPY --from=builder /app/bin/server /server
ENTRYPOINT ["/server"]
```

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t myapp:latest --push .
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance. Each item states the mechanism, not just the rule.

### Anti-Patterns

**Shell form silently breaks signals and drops CMD.** `CMD gunicorn ...` is wrapped as `/bin/sh -c '<string>'`, so `/bin/sh` becomes PID 1 and (docs) "does not pass signals" — your app never gets SIGTERM from `docker stop`, which waits the full timeout then SIGKILLs (no graceful shutdown, no error). A shell-form `ENTRYPOINT` additionally "ignores any CMD or docker run command line arguments" — a `CMD` beneath it is silently dropped. Use exec form; a setup wrapper must hand off PID 1 with `exec "$@"`.

```dockerfile
ENTRYPOINT ["gunicorn", "--bind", "0.0.0.0:8000", "app:create_app()"]   # process IS PID 1
```

```sh
#!/bin/sh
set -e
do_setup
exec "$@"        # execve replaces the shell so the app becomes PID 1
```

**Separate `apt-get update` / `install` layers install stale packages.** Docker caches a RUN by its literal command string, so a standalone `RUN apt-get update` is reused even after the upstream index moves; editing only the install line rebuilds against the stale index, silently reintroducing outdated/CVE-laden packages. Keep them in one RUN, pin versions as an extra cache-bust trigger, clean lists in the same layer.

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl=7.88.* git \
    && rm -rf /var/lib/apt/lists/*
```

### Gotchas

**`set -o pipefail` is needed for piped RUNs — but default `/bin/sh` may lack it.** POSIX `sh` reports only the last command's exit code, so `RUN wget -O- url | sh` exits 0 even when `wget` fails, committing a broken layer. dash (Debian) and ash (Alpine) don't support `-o pipefail`; set it Dockerfile-wide via `SHELL`.

```dockerfile
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN wget -qO- https://example.com/install.sh | sh
```

**Use a numeric `USER` (uid:gid), not a username.** A username must resolve against `/etc/passwd`, which `scratch` and some distroless images lack — the container fails to start. It also breaks Kubernetes `runAsNonRoot: true`: the kubelet parses the image User field **as a number** and never reads `/etc/passwd`, rejecting a string user ("container has runAsNonRoot and image has non-numeric user"). Always `USER <uid>:<gid>` matching the uid/gid you created. Canonical numbers: `65534` = traditional `nobody`; distroless `nonroot` = `65532`.

**`COPY --link --chown` requires numeric UID:GID.** `--link` copies into an isolated layer rebased on an implicit `scratch` with no `/etc/passwd`, so a named user fails with `invalid user index: -1`. Worse, behavior varies by builder: the BuildKit `docker-container` driver errors while the legacy `docker` driver may silently fall back to UID 0. Use numeric `--chown` with `--link`.

**Re-declare `ARG` inside each stage — values reset at every `FROM`.** An `ARG` before the first `FROM` is global scope, usable only by `FROM` lines, NOT inside any stage; an in-stage `ARG` is inherited only by stages built `FROM` it. A reference to an undeclared `ARG` expands to an empty string with no error, so builds succeed with a blank value.

```dockerfile
ARG VERSION=3.12
FROM python:${VERSION}-slim AS builder
ARG VERSION            # re-declare to use it inside the stage
RUN echo "Building with Python ${VERSION}"
```

**Rotating a build secret does NOT bust the cache.** BuildKit deliberately excludes secret contents from the cache key — docs: "Changing the value of a secret doesn't result in cache invalidation." After rotating a credential, `RUN --mount=type=secret` reuses its layer built with the OLD secret. Since ARG values DO participate in the cache key, pair the secret RUN with an `ARG` you bump on rotation.

```dockerfile
ARG CACHE_BUST=1
RUN --mount=type=secret,id=npmrc,target=/root/.npmrc npm ci
# docker build --secret id=npmrc,src=.npmrc --build-arg CACHE_BUST=2 .
```

**Alpine uses musl, not glibc.** Pre-compiled glibc-linked binaries (some Python C-extension wheels, certain Node native addons, some JVM tools) fail at runtime on Alpine with misleading "not found" / "no such file or directory" errors even though the file exists — musl's loader provides no `libc.so.6`. Common surprise: `pip install` succeeds but the package crashes at import. Use `debian:slim`/`ubuntu` when glibc is needed; Go: build static `CGO_ENABLED=0`; Rust: target musl.

### Idioms

**`RUN --mount=type=bind` for build-time-only inputs instead of COPY + rm.** When a file is needed only during one RUN (manifests, `.npmrc`, `requirements.txt`), `COPY` commits it to a layer permanently and a later `rm` cannot remove it from the earlier layer. A bind mount exposes the file read-only for that RUN and is never committed. Requires `# syntax=docker/dockerfile:1`.

```dockerfile
# syntax=docker/dockerfile:1
RUN --mount=type=bind,source=requirements.txt,target=/tmp/requirements.txt \
    pip install --no-cache-dir -r /tmp/requirements.txt
```

**`RUN --mount=type=cache` for package-manager caches.** A persistent on-host cache survives across builds regardless of which layers were invalidated, and never enters image layers. Two sharp edges: the cache dir defaults to `uid=0,gid=0`, so a cache mount after `USER <nonroot>` is unwritable and silently falls back to a cold install — pass matching `uid`/`gid`; and `apt` needs `sharing=locked` since its DB can't tolerate concurrent writers.

```dockerfile
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    go build -o /bin/server ./cmd/server

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends curl

USER appuser
RUN --mount=type=cache,target=/home/appuser/.cache/pip,uid=1000,gid=1000 \
    pip install -r requirements.txt
```

**`ADD --checksum` to fetch+verify remote artifacts atomically** (stable since syntax 1.6). Replaces `RUN wget ... && sha256sum -c` (which also adds a layer); verification is atomic with the fetch and fails the build on mismatch. HTTP sources use `sha256:<hash>`; Git URL sources use the commit SHA.

```dockerfile
ADD --checksum=sha256:24454f830cdb571e2c4ad15481119c43b3cafd48dd869a9b2945d1036d1dc68d \
    https://example.com/artifact.tar.gz /tmp/artifact.tar.gz
```

**Enable built-in lint with the `# check=` directive** (Dockerfile 1.8+). `docker build --check` runs build-time lint rules (JSONArgsRecommended, SecretsUsedInArgOrEnv, UndefinedVar, deprecated MAINTAINER, …) without building; `# check=error=true` fails the build on violations. When you use `error=true`, pin the syntax to a specific minor so future rule additions can't unexpectedly break the build — the one case where minor pinning beats the floating tag.

```dockerfile
# syntax=docker/dockerfile:1.8
# check=error=true
FROM python:3.12-slim
```

### Design Patterns

**Add an init (`tini` / `docker --init`) when your app forks children.** PID 1 must forward signals AND reap orphaned children via `waitpid()`. An app run directly as PID 1 that spawns subprocesses leaves their exited children as zombies, which accumulate and can exhaust the PID table, eventually causing fork failures. This complements exec-form ENTRYPOINT: exec form fixes signal delivery, but only an init reaps zombies. Bake `tini` in for portability, or use `docker run --init` with no image change.

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends tini && rm -rf /var/lib/apt/lists/*
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/bin/myapp"]
```

### Performance

**`COPY --link` avoids cache-invalidation cascades in multi-stage builds.** Without `--link`, any change to a prior layer invalidates a COPY and everything after it; with `COPY --from=<stage>`, rebuilding the source stage invalidates the runtime copy. `--link` places the result in its own layer rebased on top of prior state, so it survives changes to earlier layers. Caveat: the isolated layer has no access to the prior filesystem — destination symlinks aren't followed and `--chown` must be numeric. Apply where symlink-following into the destination isn't needed.

### Security

**Drop all capabilities, add back only what's needed; pair with `no-new-privileges`.** OWASP Docker Security Cheat Sheet: "The most secure setup is to drop all capabilities `--cap-drop all` and then add only required ones." Pair with `--security-opt=no-new-privileges`, which sets the kernel `NO_NEW_PRIVS` flag so no process can escalate via setuid/setgid binaries. Orthogonal: capabilities bound existing privileges; no-new-privileges blocks gaining new ones. Combine with non-root and `--read-only`.

```bash
docker run --cap-drop all --cap-add NET_BIND_SERVICE \
  --security-opt=no-new-privileges --read-only myapp:latest
```

**Attach SBOM and provenance attestations via buildx.** `docker buildx build` generates SLSA provenance and SPDX SBOM attestations attached to the image manifest as in-toto JSON. Min-level provenance is added by default; opt into `mode=max` for the full build definition. Attestations are inspectable without pulling the whole image (`docker buildx imagetools inspect`), Docker Scout can enforce their presence, and an SBOM attestation lets Scout reuse the precomputed SBOM instead of re-scanning.

```bash
docker buildx build --sbom=true --provenance=mode=max --push -t registry/app:1.0.0 .
```

### Currency

**Pin base images by digest (`@sha256:`), and automate the updates.** Tags are mutable: a publisher — or an attacker with registry creds — can overwrite what a tag points to, so even `python:3.12-slim` can silently change. Tag hijacking is a real supply-chain class (the March 2025 `tj-actions/changed-files` compromise overwrote tags to point at malicious code). Digest-pinning guarantees byte-identical content; the "I'll miss security updates" objection is answered by Dependabot/Renovate/Docker Scout opening PRs on new digests — an audit trail instead of silent upgrades. Keep the tag alongside the digest for readability.

```dockerfile
FROM alpine:3.21@sha256:a8560b36e8b8210634f77d9f7f9efd7ffa463e380b75e2e74aff4511df3ef88c AS base
```

**Prefer the floating syntax tag `# syntax=docker/dockerfile:1`.** Pinning the frontend to a stale minor (`1.4`) freezes the Dockerfile at that feature set and blocks bug fixes and new stable features (`ADD --checksum` in 1.6, build checks in 1.8). BuildKit resolves `:1` to the latest 1.x.x at build time. Sole exception: `# check=error=true`, where you pin a specific minor so a new lint rule can't unexpectedly fail the build.

**`npm ci --omit=dev`, not `--only=production`.** `--only=production` was deprecated when npm 7 introduced `--omit`/`--include`; it lingers as a silent alias. `--omit=dev` also sets `NODE_ENV=production` for lifecycle scripts. Never use it in a builder stage that runs `npm run build` — bundlers/TypeScript live in devDependencies.

## Troubleshooting

| Symptom | Likely cause → fix |
| --- | --- |
| "sending build context" slow | Missing/weak `.dockerignore`; large files (`node_modules`, `.git`) in context |
| Rebuilds everything, no changes | Source copied before deps; wildcard `COPY` early; dynamic RUN output |
| Image far too large | No multi-stage; heavy base; caches not cleaned in same layer; `docker history`/`dive` to find layers |
| "permission denied" writing | Volume ownership vs container UID; use `--chown`, match `useradd -u` to host |
| "connection refused" / "name not found" | Not on same network; used `localhost` not service name; app on `127.0.0.1` not `0.0.0.0` |
| Code changes not reflected | Not bind-mounted; `docker-compose up --force-recreate` / rebuild |
| Container exits immediately | `docker logs`; verify CMD/ENTRYPOINT executable + required env; missing shared libs (musl vs glibc) |
| "no space left on device" | `docker system prune -a`, `docker builder prune`, `docker volume prune`; `docker system df` |

## CI/CD (GitHub Actions)

buildx + metadata-action for tagging + gha cache + trivy scan → SARIF upload:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    permissions: { contents: read, packages: write }
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with: { registry: ghcr.io, username: "${{ github.actor }}", password: "${{ secrets.GITHUB_TOKEN }}" }
      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=semver,pattern={{version}}
            type=sha
      - uses: docker/build-push-action@v5
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
      - uses: aquasecurity/trivy-action@master
        with: { image-ref: "${{ steps.meta.outputs.tags }}", format: sarif, output: trivy-results.sarif }
      - uses: github/codeql-action/upload-sarif@v3
        with: { sarif_file: trivy-results.sarif }
```

GitLab CI equivalent: `docker:24-dind` service, `docker build --pull`, then a `trivy image --exit-code 1 --severity HIGH,CRITICAL` gate stage before pushing `latest`.

## Verification Checklists

### Build / performance

- [ ] Multi-stage; final image has no build toolchain
- [ ] Minimal + digest-pinned base (`alpine`/`slim`/distroless/`scratch`, `@sha256:`)
- [ ] Dependency manifests copied + installed BEFORE source
- [ ] `apt-get update` + `install` in one RUN with `--no-install-recommends` + list cleanup
- [ ] BuildKit cache mounts for package managers (matching `uid`/`gid` if after `USER`)
- [ ] `.dockerignore` excludes `.git`/deps/artifacts
- [ ] `# syntax=docker/dockerfile:1` (floating); `docker build --check` clean

### Security / runtime

- [ ] Numeric non-root `USER uid:gid`
- [ ] No secrets in `ENV`/`ARG`/`COPY`; build secrets via `RUN --mount=type=secret`
- [ ] Exec-form `CMD`/`ENTRYPOINT`; init (`tini`/`--init`) if the app forks children
- [ ] `HEALTHCHECK` defined; compose `depends_on` uses `condition: service_healthy`
- [ ] `--cap-drop all` + only needed caps, `--security-opt=no-new-privileges`, `--read-only`
- [ ] CVE scan in CI; SBOM + provenance attached via buildx
