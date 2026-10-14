//! Commands the stage sandbox cannot serve: a network binary with no domain
//! granted, and a resource no sandbox grant reaches. Registry installs are
//! left to `registry_domains`, which names the domains they need. Also warns
//! on `allow_write` entries inside the worktree, which loom drops, and on
//! `allow_unix_sockets` on Linux, which the sandbox ignores; a grant of every
//! Unix socket gets a note.

use crate::plan::schema::{NetworkConfig, StageDefinition};
use crate::sandbox::preflight::ACCEPT_UNIX_SOCKET_RISK_FLAG;
use crate::sandbox::TMP_SCAN_DEPTH;

use super::super::criterion_hazards::{criterion_needs_ungrantable_resource, scan, Hazard};
use super::{stage_commands, LintContext, LintFinding};

/// Install hazards `registry_domains` reports with the domains they need.
const REGISTRY_INSTALLS: [&str; 3] = ["npm install", "bun install", "cargo install"];

/// Push the findings and return the notes.
pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) -> Vec<String> {
    let plan_sandbox = &ctx.metadata.loom.sandbox;
    let mut notes = Vec::new();
    report_in_tree_grants(None, &plan_sandbox.filesystem.allow_write, out);
    check_unix_sockets(None, &plan_sandbox.network, out, &mut notes);
    for stage in &ctx.metadata.loom.stages {
        let network = stage
            .sandbox
            .network
            .as_ref()
            .unwrap_or(&plan_sandbox.network);
        check_commands(stage, network, out);
        if let Some(own_network) = &stage.sandbox.network {
            check_unix_sockets(Some(stage), own_network, out, &mut notes);
        }
        if let Some(filesystem) = &stage.sandbox.filesystem {
            report_in_tree_grants(Some(stage), &filesystem.allow_write, out);
        }
    }
    notes
}

/// Warn on a non-empty `allow_unix_sockets` on a Linux host, where the
/// sandbox cannot filter a socket by its path, and note what an
/// `allow_all_unix_sockets: true` grant costs. The grant is a note rather
/// than a warning so it never fails `--strict`: `loom run` refuses it until
/// the operator accepts it.
fn check_unix_sockets(
    stage: Option<&StageDefinition>,
    network: &NetworkConfig,
    out: &mut Vec<LintFinding>,
    notes: &mut Vec<String>,
) {
    if cfg!(target_os = "linux") && !network.allow_unix_sockets.is_empty() {
        out.push(LintFinding {
            stage_id: stage.map(|stage| stage.id.clone()),
            message: "sandbox.network.allow_unix_sockets has no effect on Linux: the sandbox \
                      cannot filter a Unix socket by its path, so every socket stays denied; \
                      only `allow_all_unix_sockets: true` lifts that filter"
                .to_string(),
            error_in_v2: false,
        });
    }
    if network.allow_all_unix_sockets {
        let whose = match stage {
            Some(stage) => format!("stage `{}` sets", stage.id),
            None => "the plan's `sandbox:` block sets".to_string(),
        };
        notes.push(format!(
            "{whose} sandbox.network.allow_all_unix_sockets: true. `loom run` refuses to start \
             without `{ACCEPT_UNIX_SOCKET_RISK_FLAG}`. {}",
            reachable_sockets()
        ));
    }
}

/// Which host sockets a session granted every Unix socket still reaches.
fn reachable_sockets() -> String {
    if cfg!(target_os = "linux") {
        format!(
            "loom hides /run, /var/run, the ssh and gpg agents, tmux, X11, the loom daemon and \
             the sockets found up to {TMP_SCAN_DEPTH} levels below /tmp at session start; its \
             sessions reach every other socket, such as those under /var/snap, /var/lib or \
             $HOME, a /tmp socket created later or nested deeper, and the restarted daemon's \
             socket in a session that outlives a daemon restart"
        )
    } else {
        "loom hides no host socket on this platform, so its sessions reach every Unix socket \
         on the host"
            .to_string()
    }
}

/// Warn on every `allow_write` entry that names a concrete path inside the
/// worktree. The session's working directory is writable already, and
/// `sandbox::build_settings` drops such an entry: emitted, it would become a
/// bind mount that git cannot unlink or rename (`Device or resource busy`).
fn report_in_tree_grants(
    stage: Option<&StageDefinition>,
    allow_write: &[String],
    out: &mut Vec<LintFinding>,
) {
    for entry in allow_write.iter().map(|entry| entry.trim()) {
        if names_in_tree_path(entry) {
            out.push(LintFinding {
                stage_id: stage.map(|stage| stage.id.clone()),
                message: format!(
                    "allow_write `{entry}` is inside the worktree, which is already writable; \
                     loom drops it"
                ),
                error_in_v2: false,
            });
        }
    }
}

/// A relative, non-glob path that climbs nowhere and does not pass through
/// the `.loom/work` or `.work` state symlink (whose target lies outside the
/// worktree). Absolute, `~/` and `$VAR` entries cannot be placed before the
/// worktree exists, so they are left alone.
fn names_in_tree_path(entry: &str) -> bool {
    !entry.is_empty()
        && !entry.starts_with(['/', '~', '$'])
        && !entry.contains(['*', '?', '[', '{'])
        && !entry.split('/').any(|part| part == "..")
        && entry != ".work"
        && !entry.starts_with(".loom/")
        && !entry.starts_with(".work/")
}

fn check_commands(stage: &StageDefinition, network: &NetworkConfig, out: &mut Vec<LintFinding>) {
    let no_domains = network.allowed_domains.is_empty() && network.additional_domains.is_empty();
    for command in stage_commands(stage) {
        if no_domains {
            for hazard in scan(command.text, false) {
                if let Hazard::Network(tool) = hazard {
                    if REGISTRY_INSTALLS.contains(&tool) {
                        continue;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::{FilesystemConfig, LoomConfig, LoomMetadata};

    fn owned(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|entry| entry.to_string()).collect()
    }

    fn in_tree_findings(plan: &[&str], stage: &[&str]) -> Vec<LintFinding> {
        let mut metadata = LoomMetadata {
            loom: LoomConfig {
                version: 2,
                stages: vec![StageDefinition {
                    id: "s1".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        };
        metadata.loom.sandbox.filesystem.allow_write = owned(plan);
        metadata.loom.stages[0].sandbox.filesystem = Some(FilesystemConfig {
            allow_write: owned(stage),
            ..FilesystemConfig::default()
        });
        let ctx = LintContext {
            metadata: &metadata,
            repo_root: None,
        };
        let mut out = Vec::new();
        check(&ctx, &mut out);
        out.retain(|finding| finding.message.contains("is inside the worktree"));
        out
    }

    #[test]
    fn only_concrete_in_tree_grants_warn() {
        let findings = in_tree_findings(
            &[
                "dist",
                "src/**",
                "/abs/cache",
                "~/cache",
                "${HOME}/cache",
                "../sibling",
                ".loom/work/handoffs",
                ".work",
            ],
            &["vitest.config.ts"],
        );

        let expected = |stage_id: Option<&str>, entry: &str| LintFinding {
            stage_id: stage_id.map(str::to_string),
            message: format!(
                "allow_write `{entry}` is inside the worktree, which is already writable; \
                 loom drops it"
            ),
            error_in_v2: false,
        };
        assert_eq!(
            findings,
            vec![
                expected(None, "dist"),
                expected(Some("s1"), "vitest.config.ts")
            ]
        );
    }

    /// The socket findings and notes for a plan whose own network is `plan`
    /// and whose one stage, `s1`, overrides it with `stage`.
    fn socket_lints(
        plan: NetworkConfig,
        stage: Option<NetworkConfig>,
    ) -> (Vec<LintFinding>, Vec<String>) {
        let mut metadata = LoomMetadata {
            loom: LoomConfig {
                version: 2,
                stages: vec![StageDefinition {
                    id: "s1".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        };
        metadata.loom.sandbox.network = plan;
        metadata.loom.stages[0].sandbox.network = stage;
        let ctx = LintContext {
            metadata: &metadata,
            repo_root: None,
        };
        let mut out = Vec::new();
        let notes = check(&ctx, &mut out);
        out.retain(|finding| finding.message.contains("unix_sockets"));
        (out, notes)
    }

    #[test]
    fn a_path_scoped_socket_grant_warns_on_linux() {
        let scoped = NetworkConfig {
            allow_unix_sockets: owned(&["/tmp/app.sock"]),
            ..NetworkConfig::default()
        };

        let (findings, notes) = socket_lints(NetworkConfig::default(), Some(scoped));

        assert!(notes.is_empty(), "{notes:?}");
        if cfg!(target_os = "linux") {
            assert_eq!(findings.len(), 1, "{findings:?}");
            assert_eq!(findings[0].stage_id.as_deref(), Some("s1"));
            assert!(findings[0].message.contains("no effect on Linux"));
            assert!(!findings[0].error_in_v2);
        } else {
            assert!(findings.is_empty(), "{findings:?}");
        }
    }

    #[test]
    fn a_grant_of_every_socket_is_a_note_naming_the_run_flag() {
        let all = NetworkConfig {
            allow_all_unix_sockets: true,
            ..NetworkConfig::default()
        };

        let (findings, notes) = socket_lints(all.clone(), None);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("the plan's `sandbox:` block sets"));
        assert!(notes[0].contains(ACCEPT_UNIX_SOCKET_RISK_FLAG), "{notes:?}");
        if cfg!(target_os = "linux") {
            let depth = format!("up to {TMP_SCAN_DEPTH} levels below /tmp");
            for reachable in [
                "/var/snap",
                "/var/lib",
                "$HOME",
                "nested deeper",
                depth.as_str(),
            ] {
                assert!(notes[0].contains(reachable), "{reachable}: {notes:?}");
            }
            assert!(notes[0].contains("outlives a daemon restart"), "{notes:?}");
        } else {
            assert!(
                notes[0].contains("every Unix socket on the host"),
                "{notes:?}"
            );
        }

        let (_, notes) = socket_lints(NetworkConfig::default(), Some(all));
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("stage `s1` sets"), "{notes:?}");
    }
}
