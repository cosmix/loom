# PLAN: Complex Dependency Test

A test plan with diamond dependencies and multiple paths.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: init
      name: "Initialize"
      description: "Project initialization"
      dependencies: []
      acceptance:
        - "echo init"
    - id: models
      name: "Data Models"
      description: "Create data models"
      dependencies: [init]
      parallel_group: "core"
      files:
        - "src/models/*.rs"
    - id: database
      name: "Database Schema"
      description: "Setup database"
      dependencies: [init]
      parallel_group: "core"
      files:
        - "migrations/*.sql"
    - id: api
      name: "API Layer"
      description: "Build API endpoints"
      dependencies: [models, database]
      acceptance:
        - "cargo test --lib api"
      files:
        - "src/api/*.rs"
    - id: tests
      name: "Integration Tests"
      dependencies: [api]
      acceptance:
        - "cargo test"
```

<!-- END loom METADATA -->
