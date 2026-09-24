//! Commands the stage sandbox cannot serve: a network binary with no domain
//! granted, and a resource no sandbox grant reaches.

use crate::plan::schema::{NetworkConfig, StageDefinition};

use super::super::criterion_hazards::{criterion_needs_ungrantable_resource, scan, Hazard};
use super::{stage_commands, LintContext, LintFinding};

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    let plan_network = &ctx.metadata.loom.sandbox.network;
    for stage in &ctx.metadata.loom.stages {
        let network = stage.sandbox.network.as_ref().unwrap_or(plan_network);
        check_commands(stage, network, out);
    }
}

fn check_commands(stage: &StageDefinition, network: &NetworkConfig, out: &mut Vec<LintFinding>) {
    let no_domains = network.allowed_domains.is_empty() && network.additional_domains.is_empty();
    for command in stage_commands(stage) {
        if no_domains {
            for hazard in scan(command.text, false) {
                if let Hazard::Network(tool) = hazard {
                    let problem = format!(
                        "runs `{tool}` while the stage's sandbox allows no network domain; add \
                         the host it reaches to `sandbox.network.allowed_domains` or \
                         `sandbox.network.additional_domains`"
                    );
                    out.push(LintFinding::in_stage(
                        stage,
                        command.describe(&problem),
                        true,
                    ));
                }
            }
        }
        if let Some(what) = criterion_needs_ungrantable_resource(command.text) {
            let problem = format!(
                "invokes `{what}`, which needs shared .loom state or a host daemon that no \
                 sandbox grant reaches"
            );
            out.push(LintFinding::in_stage(
                stage,
                command.describe(&problem),
                true,
            ));
        }
    }
}
