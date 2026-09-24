# PLAN: Version 2 Fixture

A `version: 2` plan with every stage kind: a knowledge bootstrap, one standard stage
carrying a behavioural contract, an integration-verify stage that runs the full test
suite, and a knowledge distillation. `loom plan verify --strict` must accept it.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  stages:
    - id: knowledge-bootstrap
      name: "Knowledge bootstrap"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      description: "Re-verify the knowledge topics the later stages are briefed from."
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"

    - id: add-greeting
      name: "Add a greeting"
      stage_type: standard
      working_dir: "."
      dependencies: ["knowledge-bootstrap"]
      description: "Greet a user by name."
      contracts:
        - id: greets-by-name
          file: "loom/tests/greeting.rs"
          test: "greets_by_name"
          runner: cargo-test
          scenario: "a greeting is built for the user named Ada"
          rejects: "a greeting that ignores the name and always says hello world"
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --test greeting"

    - id: integration-verify
      name: "Integration verification"
      stage_type: integration-verify
      working_dir: "."
      dependencies: ["add-greeting"]
      description: "Verify the merged tree."
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --all-targets"

    - id: knowledge-distill
      name: "Knowledge distillation"
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: "Curate the stage memories into knowledge."
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
```

<!-- END loom METADATA -->
