# PLAN: Parallel Execution Test

A test plan with parallel stage execution.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: foundation
      name: "Foundation"
      description: "Base setup"
      dependencies: []
    - id: frontend
      name: "Frontend Work"
      dependencies: [foundation]
      parallel_group: "implementation"
      files:
        - "src/frontend/*.rs"
    - id: backend
      name: "Backend Work"
      dependencies: [foundation]
      parallel_group: "implementation"
      files:
        - "src/backend/*.rs"
    - id: integration
      name: "Integration"
      dependencies: [frontend, backend]
      acceptance:
        - "cargo test --test integration"
```

<!-- END loom METADATA -->
