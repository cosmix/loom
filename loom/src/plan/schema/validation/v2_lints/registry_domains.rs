//! Package-registry commands in a sandboxed stage: `bunx`, `npm install`,
//! `cargo install`, `uv sync` and the like fetch from a registry host, and the
//! stage's sandbox must allow it. A command the confined completion runner
//! executes has no network at all, so it is reported whatever the sandbox allows.

use crate::plan::schema::{CommandConfinement, StageDefinition};

use super::super::shell_lex::Word;
use super::{sandboxed_commands, visit_argvs, LintContext, LintFinding, StageCommand};

pub(super) type Domains = &'static [&'static str];

const NPM: Domains = &["registry.npmjs.org"];
const CRATES: Domains = &["crates.io", "index.crates.io", "static.crates.io"];
const PYPI: Domains = &["pypi.org", "files.pythonhosted.org"];
const GO_PROXY: Domains = &["proxy.golang.org"];

/// `xargs` options that take their value as the next word.
const XARGS_VALUE_OPTIONS: [&str; 8] = ["-n", "-I", "-P", "-d", "-L", "-a", "-E", "-s"];

/// The tool label and registry domains `argv` needs, or `None` when it fetches
/// nothing from a registry. An `xargs` prefix and its options are skipped, so
/// `xargs -0 bunx x` reads as `bunx x`.
pub(super) fn registry_need(argv: &[&Word]) -> Option<(String, Domains)> {
    let (first, rest) = skip_xargs(argv).split_first()?;
    let name = first.command_name();
    let rest: Vec<&str> = rest.iter().map(|word| word.value.as_str()).collect();
    if matches!(name, "python" | "python3") {
        let installs = rest.starts_with(&["-m", "pip", "install"]);
        return installs.then(|| (format!("{name} -m pip install"), PYPI));
    }
    let subs: Vec<&str> = rest
        .iter()
        .copied()
        .filter(|arg| !arg.starts_with(['-', '+']))
        .collect();
    // The number of subcommand words that belong to the tool's label.
    let used = match (name, subs.as_slice()) {
        ("bunx" | "npx" | "uvx", _) => 0,
        ("bun", ["x", ..]) | ("pnpm" | "yarn", ["dlx", ..]) => 1,
        ("npm" | "bun" | "pnpm" | "yarn", ["install" | "i" | "ci" | "add", ..]) => 1,
        ("cargo", ["install" | "fetch", ..]) => 1,
        ("uv", ["sync" | "add", ..]) | ("pip" | "pip3", ["install", ..]) => 1,
        ("uv", ["pip", "install", ..]) => 2,
        ("go", ["get", ..]) => 1,
        ("go", ["mod", "download", ..]) => 2,
        _ => return None,
    };
    let domains = match name {
        "cargo" => CRATES,
        "uv" | "uvx" | "pip" | "pip3" => PYPI,
        "go" => GO_PROXY,
        _ => NPM,
    };
    let label = std::iter::once(name)
        .chain(subs.iter().copied().take(used))
        .collect::<Vec<_>>()
        .join(" ");
    Some((label, domains))
}

fn skip_xargs<'a, 'w>(argv: &'a [&'w Word]) -> &'a [&'w Word] {
    if argv.first().map(|word| word.command_name()) != Some("xargs") {
        return argv;
    }
    let mut index = 1;
    while let Some(word) = argv.get(index).filter(|word| word.value.starts_with('-')) {
        let takes_value = XARGS_VALUE_OPTIONS.contains(&word.value.as_str());
        index += if takes_value { 2 } else { 1 };
    }
    &argv[index.min(argv.len())..]
}

/// Whether the domain pattern `pattern` (`*`, `*.suffix` or an exact host, the
/// forms `validate_domain_pattern` accepts) allows `host`; ASCII
/// case-insensitive.
pub(super) fn allows(pattern: &str, host: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(suffix) => host.ends_with(&format!(".{suffix}")),
        None => pattern == "*" || pattern == host,
    }
}

/// The domains of `needed` that no pattern in `allowed` covers.
pub(super) fn missing_domains(allowed: &[&str], needed: Domains) -> Vec<&'static str> {
    needed
        .iter()
        .copied()
        .filter(|host| !allowed.iter().any(|pattern| allows(pattern, host)))
        .collect()
}

/// The network patterns the stage's sandbox allows, or `None` when its sandbox
/// is disabled. A stage's own `network` replaces the plan's whole, as
/// `sandbox::config::merge_config` resolves it.
pub(super) fn allowed_patterns<'a>(
    ctx: &LintContext<'a>,
    stage: &'a StageDefinition,
) -> Option<Vec<&'a str>> {
    let plan_sandbox = &ctx.metadata.loom.sandbox;
    if !stage.sandbox.enabled.unwrap_or(plan_sandbox.enabled) {
        return None;
    }
    let network = stage
        .sandbox
        .network
        .as_ref()
        .unwrap_or(&plan_sandbox.network);
    Some(
        network
            .allowed_domains
            .iter()
            .chain(&network.additional_domains)
            .map(String::as_str)
            .collect(),
    )
}

/// Label prefixes of the commands `loom stage complete` runs in the confined
/// runner, which has no network whatever the plan's `allowed_domains` says
/// (`verify/criteria/confine.rs`). `after_stage` checks are not among them.
const RUNNER_LABELS: [&str; 4] = [
    "Acceptance criterion",
    "Setup command",
    "Wiring test",
    "dead_code_check",
];

/// Whether `command` runs in the confined runner of a stage whose effective
/// `command_confinement` is `confined`.
fn runs_confined(
    ctx: &LintContext<'_>,
    stage: &StageDefinition,
    command: &StageCommand<'_>,
) -> bool {
    let confinement = stage
        .sandbox
        .command_confinement
        .unwrap_or(ctx.metadata.loom.sandbox.command_confinement);
    confinement == CommandConfinement::Confined
        && RUNNER_LABELS
            .iter()
            .any(|prefix| command.label.starts_with(prefix))
}

/// The finding text for a registry command, or `None` when nothing is wrong.
/// In the confined runner the fetch fails whatever the session may reach;
/// elsewhere it fails only for domains the stage's sandbox does not allow.
fn problem(
    tool: &str,
    needed: Domains,
    allowed: Option<&[&str]>,
    in_runner: bool,
) -> Option<String> {
    if in_runner {
        return Some(format!(
            "runs `{tool}`, which needs {}; completion runs this command in the confined \
             runner, which has no network whatever `allowed_domains` says: install it from a \
             `provision` entry and call the installed binary",
            needed.join(", ")
        ));
    }
    let missing = missing_domains(allowed?, needed);
    (!missing.is_empty()).then(|| {
        format!(
            "runs `{tool}`, which needs {}; the stage's sandbox does not allow {}: add it to \
             `sandbox.network.allowed_domains` or `additional_domains`, or install from a plan \
             `provision` entry",
            needed.join(", "),
            missing.join(", ")
        )
    })
}

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    for stage in &ctx.metadata.loom.stages {
        let allowed = allowed_patterns(ctx, stage);
        for command in sandboxed_commands(stage) {
            let in_runner = runs_confined(ctx, stage, &command);
            let mut reported: Vec<String> = Vec::new();
            visit_argvs(command.text, 0, &mut |argv| {
                let Some((tool, needed)) = registry_need(argv) else {
                    return;
                };
                let Some(problem) = problem(&tool, needed, allowed.as_deref(), in_runner) else {
                    return;
                };
                if reported.contains(&tool) {
                    return;
                }
                out.push(LintFinding::in_stage(
                    stage,
                    command.describe(&problem),
                    true,
                ));
                reported.push(tool);
            });
        }
    }
}
