//! Plan texts for the criteria-authoring contracts. Each builder returns a
//! whole plan file whose metadata passes `plan::schema::validate`, so the
//! plan lints and the dry run see it: version 1 plans carry no contracts,
//! harness, reachable or literal wiring, and version 2 plans carry the
//! knowledge and integration-verify bookends.

#![allow(dead_code)]

/// A YAML double-quoted scalar for `text` (JSON escapes are YAML escapes).
pub fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("a string serialises")
}

/// One stage entry under `stages:`; `extra` holds further keys, each line
/// indented by six spaces.
pub fn stage(
    id: &str,
    stage_type: &str,
    deps: &[&str],
    acceptance: &[&str],
    extra: &str,
) -> String {
    let deps: Vec<String> = deps.iter().map(|dep| quoted(dep)).collect();
    let acceptance: String = acceptance
        .iter()
        .map(|command| format!("        - {}\n", quoted(command)))
        .collect();
    format!(
        "    - id: {id}\n      name: \"{id}\"\n      stage_type: {stage_type}\n      \
         working_dir: \".\"\n      dependencies: [{}]\n      summary: \"Stage {id}.\"\n      \
         description: \"Stage {id} of the fixture plan.\"\n      acceptance:\n{acceptance}{extra}",
        deps.join(", ")
    )
}

/// A dependency-free standard stage of a version 1 plan.
pub fn standard(id: &str, acceptance: &[&str], extra: &str) -> String {
    stage(id, "standard", &[], acceptance, extra)
}

fn plan(version: u32, loom_extra: &str, stages: &str) -> String {
    format!(
        "# PLAN: Criteria Authoring Fixture\n\n---\n\n<!-- loom METADATA -->\n\n```yaml\n\
         loom:\n  version: {version}\n{loom_extra}  stages:\n{stages}```\n\n\
         <!-- END loom METADATA -->\n"
    )
}

/// A version 1 plan holding `stages` (concatenated [`stage`] entries).
pub fn v1_plan(stages: &str) -> String {
    plan(1, "", stages)
}

/// A version 2 plan: a knowledge bootstrap, the standard stage `add-greeting`
/// carrying one contract with `standard_extra` appended to its keys, an
/// integration-verify stage depending on it and a knowledge distillation.
/// `loom_extra` is spliced in under `loom:` (two-space indented keys).
pub fn v2_plan(loom_extra: &str, standard_extra: &str) -> String {
    let contract = "      contracts:\n        - id: greets-by-name\n          \
                    file: \"tests/greeting.rs\"\n          test: \"greets_by_name\"\n          \
                    runner: cargo-test\n          scenario: \"a greeting is built for Ada\"\n          \
                    rejects: \"a greeting that ignores the name\"\n";
    let stages = [
        stage(
            "knowledge-bootstrap",
            "knowledge",
            &[],
            &["loom knowledge check"],
            "",
        ),
        stage(
            "add-greeting",
            "standard",
            &["knowledge-bootstrap"],
            &["cargo test --test greeting"],
            &format!("{contract}{standard_extra}"),
        ),
        stage(
            "integration-verify",
            "integration-verify",
            &["add-greeting"],
            &["cargo test --all-targets"],
            "",
        ),
        stage(
            "knowledge-distill",
            "knowledge-distill",
            &["integration-verify"],
            &["loom knowledge check"],
            "",
        ),
    ];
    plan(2, loom_extra, &stages.concat())
}

/// A `wiring:` list with one entry whose pattern is `pattern`; `literal`
/// marks it `literal: true` (version 2 only).
pub fn wiring(pattern: &str, literal: bool) -> String {
    let literal = if literal {
        "          literal: true\n"
    } else {
        ""
    };
    format!(
        "      wiring:\n        - source: \"src/lib.rs\"\n          pattern: '{}'\n          \
         description: \"the item is wired in\"\n{literal}",
        pattern.replace('\'', "''")
    )
}

/// `artifacts:` naming one new source file.
pub const ARTIFACTS: &str = "      artifacts: [\"src/greeting.rs\"]\n";

/// A one-line `consumer_deferred:` reason.
pub fn consumer_deferred(reason: &str) -> String {
    format!("      consumer_deferred: {}\n", quoted(reason))
}

/// A `reachable:` list with one entry.
pub const REACHABLE: &str = "      reachable:\n        - symbol: \"greet\"\n          \
                             from: \"main\"\n          description: \"main greets the user\"\n";

/// The `loom.provision` key with one entry running `command` in `.`.
pub fn provision(command: &str) -> String {
    format!(
        "  provision:\n    - working_dir: \".\"\n      command: {}\n",
        quoted(command)
    )
}
