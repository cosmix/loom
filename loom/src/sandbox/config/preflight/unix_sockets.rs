//! `loom run`'s acknowledgement of a stage granted every Unix socket.
//!
//! `sandbox.network.allow_all_unix_sockets` lets a stage's sessions create and
//! connect to Unix sockets. The capsule hides the host's own sockets
//! (`sandbox::hidden_socket_paths`), but not every one: a run that grants it
//! starts only when the operator accepts that residue for this run. The
//! acknowledgement is a flag, never persisted, so each `loom run` repeats it.

use super::{merge_config, origin_label};
use crate::models::stage::Stage;
use crate::plan::schema::SandboxConfig;
use crate::sandbox::{grants_all_unix_sockets, TMP_SCAN_DEPTH};

/// The `loom run` flag that accepts the grant for one run.
pub(crate) const ACCEPT_UNIX_SOCKET_RISK_FLAG: &str = "--accept-unix-socket-risk";

/// What a stage granted every Unix socket can still reach.
fn residual_risk() -> String {
    if cfg!(target_os = "linux") {
        format!(
            "loom hides /run, /var/run, the ssh and gpg agents, tmux, X11, the loom daemon and \
             the sockets it finds up to {TMP_SCAN_DEPTH} levels below /tmp when a session \
             starts; every other socket stays reachable, such as those under /var/snap, \
             /var/lib or $HOME, a /tmp socket created later or nested deeper, and the \
             restarted daemon's socket in a session that outlives a daemon restart"
        )
    } else {
        "on this platform loom hides no host socket, so every Unix socket on the host stays \
         reachable"
            .to_string()
    }
}

/// One refusal per stage whose merged sandbox grants every Unix socket,
/// naming where the setting came from; none once `accepted`.
pub(crate) fn unix_socket_grant_refusals(
    plan: &SandboxConfig,
    stages: &[Stage],
    accepted: bool,
) -> Vec<String> {
    if accepted {
        return Vec::new();
    }
    let residual = residual_risk();
    stages
        .iter()
        .filter(|stage| {
            let merged = merge_config(plan, &stage.sandbox, stage.stage_type, &stage.implementers);
            grants_all_unix_sockets(&merged)
        })
        .map(|stage| {
            format!(
                "stage `{}`: sandbox.network.allow_all_unix_sockets=true (set by {}) lets its \
                 sessions create and connect to Unix sockets; {residual}. Pass \
                 `{ACCEPT_UNIX_SOCKET_RISK_FLAG}` to `loom run` to accept that for this run.",
                stage.id,
                origin_label(stage.sandbox.network.is_some())
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::{NetworkConfig, StageSandboxConfig};

    fn granting(all: bool) -> NetworkConfig {
        NetworkConfig {
            allow_all_unix_sockets: all,
            ..NetworkConfig::default()
        }
    }

    fn stage(id: &str, network: Option<NetworkConfig>) -> Stage {
        Stage {
            id: id.to_string(),
            sandbox: StageSandboxConfig {
                network,
                ..StageSandboxConfig::default()
            },
            ..Stage::default()
        }
    }

    #[test]
    fn a_stage_granted_every_socket_is_refused_until_the_risk_is_accepted() {
        let plan = SandboxConfig::default();
        let stages = [
            stage("plain", None),
            stage("sockets", Some(granting(true))),
            stage("narrow", Some(granting(false))),
        ];

        let refusals = unix_socket_grant_refusals(&plan, &stages, false);

        assert_eq!(refusals.len(), 1, "{refusals:?}");
        let refusal = &refusals[0];
        assert!(
            refusal.starts_with("stage `sockets`: sandbox.network.allow_all_unix_sockets=true"),
            "{refusal}"
        );
        assert!(refusal.contains("(set by the stage's own `sandbox:` override)"));
        assert!(refusal.contains(ACCEPT_UNIX_SOCKET_RISK_FLAG), "{refusal}");
        assert!(refusal.contains("stays reachable"), "{refusal}");
        if cfg!(target_os = "linux") {
            let depth = format!("up to {TMP_SCAN_DEPTH} levels below /tmp");
            for reachable in [
                "/var/snap",
                "/var/lib",
                "$HOME",
                "nested deeper",
                "created later",
            ] {
                assert!(refusal.contains(reachable), "{reachable}: {refusal}");
            }
            assert!(refusal.contains(&depth), "{refusal}");
            assert!(refusal.contains("outlives a daemon restart"), "{refusal}");
        }
        assert!(unix_socket_grant_refusals(&plan, &stages, true).is_empty());
    }

    #[test]
    fn a_plan_level_grant_is_named_as_the_plans_and_a_stage_override_lifts_it() {
        let plan = SandboxConfig {
            network: granting(true),
            ..SandboxConfig::default()
        };
        let stages = [
            stage("inherits", None),
            stage("narrow", Some(granting(false))),
        ];

        let refusals = unix_socket_grant_refusals(&plan, &stages, false);

        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert!(refusals[0].starts_with("stage `inherits`:"), "{refusals:?}");
        assert!(refusals[0].contains("(set by the plan's `sandbox:` block)"));
    }

    #[test]
    fn the_flag_named_in_the_refusal_is_the_one_loom_run_accepts() {
        use clap::Parser;
        let parsed = crate::cli::Cli::try_parse_from(["loom", "run", ACCEPT_UNIX_SOCKET_RISK_FLAG]);
        assert!(parsed.is_ok(), "{:?}", parsed.err());
    }
}
