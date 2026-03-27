---
name: loom-docker
description: "Creates and optimizes Docker configurations including Dockerfiles, docker-compose files, and container orchestration. Covers multi-stage builds, layer optimization, security hardening, networking, volumes, and debugging. Use when the user needs to containerize an application, optimize Docker images, configure docker-compose services, or troubleshoot container issues."
allowed-tools: Read, Grep, Glob, Edit, Write, Bash
trigger-keywords: docker, container, dockerfile, image, compose, registry, build, layer, cache, multi-stage, volume, network, port, environment, containerize, orchestration, push, pull, tag, alpine, slim, distroless
---

# Docker

## Overview

This skill provides Docker expertise for all container-related tasks including writing optimized Dockerfiles, docker-compose files, security hardening, image optimization, and debugging. It covers the full lifecycle from initial containerization strategy through production deployment.

## Instructions

### 1. Analyze Application Requirements

Before creating container configurations, the agent should identify:

- **Runtime Dependencies**: Language runtime, system libraries, native extensions
- **Build vs Runtime Separation**: Plan multi-stage builds to separate build tools from runtime
- **Configuration Management**: Environment variables, secrets, config files
- **Data Persistence**: Stateful vs stateless components, volume mount points
- **Network Requirements**: Ports, service dependencies, external integrations
- **Deployment Target**: Development, staging, or production environments

### 2. Write Efficient Dockerfiles

#### Base Image Selection

- Prefer official language/OS images from Docker Hub
- Use Alpine (-alpine), Slim (-slim), or Distroless for production
- Pin specific version tags (e.g., `python:3.12-alpine`), never use `latest`
- Use multi-stage builds to separate builder and runtime stages

#### Layer Optimization

- Place least-changing instructions first (base image, system deps)
- Chain related RUN commands with `&&` to reduce layer count
- Clean package manager caches in the same RUN command they are created
- Copy dependency files before source code for better caching
- Use `--mount=type=cache` for package manager caches with BuildKit

#### Security Best Practices

- Create and switch to a non-root user
- Use read-only root filesystem where possible
- Use `--mount=type=secret` for build-time secrets; never embed secrets in ENV, ARG, or COPY
- Scan images with vulnerability scanners (trivy, grype, Docker Scout) in CI
- Drop unnecessary Linux capabilities; never use `--privileged` without justification

### 3. Configure Docker Compose

- Use `depends_on` with `condition: service_healthy` for service dependencies
- Define HEALTHCHECK commands for critical services
- Set appropriate restart policies (`unless-stopped`, `on-failure`)
- Define named networks for service isolation; mark internal services without external ports
- Use named volumes for data persistence; bind mounts for development only
- Use `.env` files for local development; docker secrets or external managers for production
- Use `${VAR:-default}` for variable interpolation with defaults

### 4. Language-Specific Patterns

- **Python**: Use `pip wheel` for cached compiled deps; `--no-cache-dir` to prevent pip cache in layers; `python:3.x-slim` for production
- **Node.js**: Copy `package*.json` before source; use `npm ci` for reproducible builds; remove devDependencies in production
- **Rust**: Use `cargo-chef` for dependency caching; build with `--release`; copy only the binary to a distroless runtime stage
- **Go**: Build statically with `CGO_ENABLED=0`; use `scratch` or distroless for runtime; copy only the binary

### 5. Development vs Production

**Development**: bind mounts for live reload, include debugging tools, verbose logging, expose additional ports.

**Production**: named volumes for persistence, minimal images with only runtime deps, restricted ports and capabilities, structured logging to stdout/stderr, no SSH or debugging tools in final image.

## Best Practices

1. **Use Official Base Images** from trusted sources
2. **Multi-Stage Builds** to separate build and runtime environments
3. **Minimize Layers** by combining related commands with `&&`
4. **Run as Non-Root** user for security
5. **Use .dockerignore** to exclude unnecessary files from build context
6. **Pin Versions** on base images and dependencies
7. **Add HEALTHCHECK** instructions for container health monitoring
8. **Scan Regularly** with vulnerability scanners
9. **Clear Caches** in the same layer they are created
10. **Document Non-Obvious Decisions** with comments in Dockerfiles

## Examples

### Example 1: Multi-Stage Python Dockerfile

```dockerfile
# Build stage
FROM python:3.12-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*
COPY requirements.txt .
RUN pip wheel --no-cache-dir --no-deps --wheel-dir /app/wheels -r requirements.txt

# Runtime stage
FROM python:3.12-slim AS runtime
RUN groupadd --gid 1000 appgroup && \
    useradd --uid 1000 --gid appgroup --shell /bin/bash --create-home appuser
WORKDIR /app
COPY --from=builder /app/wheels /wheels
RUN pip install --no-cache-dir /wheels/* && rm -rf /wheels
COPY --chown=appuser:appgroup . .
USER appuser
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1
CMD ["gunicorn", "--bind", "0.0.0.0:8000", "--workers", "4", "app:create_app()"]
```

### Example 2: Rust Dockerfile with Cargo Chef

```dockerfile
# Chef stage - plan dependencies
FROM rust:1.75-alpine AS chef
RUN apk add --no-cache musl-dev && cargo install cargo-chef
WORKDIR /app

# Planner stage - analyze dependencies
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage - build dependencies then app
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release && strip target/release/myapp

# Runtime stage - minimal distroless image
FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/myapp /usr/local/bin/myapp
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/myapp"]
```

### Example 3: Docker Compose for Development

```yaml
version: "3.8"

services:
  app:
    build:
      context: .
      dockerfile: Dockerfile.dev
    ports:
      - "3000:3000"
    volumes:
      - .:/app
      - /app/node_modules
    environment:
      - NODE_ENV=development
      - DATABASE_URL=postgres://user:pass@db:5432/myapp
      - REDIS_URL=redis://cache:6379
    depends_on:
      db:
        condition: service_healthy
      cache:
        condition: service_started
    networks:
      - app-network

  db:
    image: postgres:16-alpine
    volumes:
      - postgres_data:/var/lib/postgresql/data
    environment:
      POSTGRES_USER: user
      POSTGRES_PASSWORD: pass
      POSTGRES_DB: myapp
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U user -d myapp"]
      interval: 5s
      timeout: 5s
      retries: 5
    networks:
      - app-network

  cache:
    image: redis:7-alpine
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data
    networks:
      - app-network

volumes:
  postgres_data:
  redis_data:

networks:
  app-network:
    driver: bridge
```

### Example 4: .dockerignore

```gitignore
.git
.gitignore
node_modules
__pycache__
*.pyc
.venv
dist
build
.idea
.vscode
coverage
.pytest_cache
.env
.env.local
.env.*.local
Dockerfile*
docker-compose*
*.md
!README.md
.DS_Store
*.log
tmp
```

## Troubleshooting

### Build Context Too Large

Add a comprehensive `.dockerignore`. Check for large files accidentally included (node_modules, .git, build artifacts). Build from a subdirectory if needed: `docker build -f Dockerfile.app ./app`.

### Layer Caching Not Working

Ensure dependency files are copied before source code. Avoid wildcard COPY at the start. Check for commands with dynamic output (dates, random values). Use BuildKit cache mounts for package managers.

### Image Size Too Large

Use multi-stage builds. Choose smaller base images (alpine, slim, distroless). Clean caches in the same RUN command: `apt-get install ... && rm -rf /var/lib/apt/lists/*`. Use `dive` to analyze layer contents.

### Permission Denied in Container

Check volume ownership matches container user. Use `--chown=user:group` in COPY instructions. Create user with specific UID/GID matching host. Initialize volumes with correct ownership in entrypoint.

### Container Cannot Connect to Other Services

Verify services are on the same Docker network. Use service names for DNS, not localhost. Ensure the service listens on `0.0.0.0`, not `127.0.0.1`. Check `depends_on` and EXPOSE in Dockerfile.

### Container Exits Immediately

Check logs with `docker logs <container>`. Run interactively: `docker run -it myapp /bin/sh`. Verify CMD/ENTRYPOINT is correct and executable. Check for missing environment variables or shared libraries.

## Quick Reference Commands

```bash
# Build
docker build -t myapp:latest .
docker build -f Dockerfile.prod -t myapp:prod .
docker buildx build --platform linux/amd64,linux/arm64 -t myapp:latest --push .

# Run
docker run -d -p 8080:8080 --name myapp myapp:latest
docker run -it --rm myapp:latest /bin/sh
docker run --env-file .env myapp:latest

# Compose
docker-compose up -d
docker-compose down -v
docker-compose logs -f app
docker-compose exec app /bin/sh

# Inspect and Debug
docker ps
docker logs -f <container>
docker exec -it <container> /bin/sh
docker stats
docker run --rm -it --entrypoint /bin/sh myapp:latest

# Clean
docker system prune -a
docker volume prune
docker builder prune
```
