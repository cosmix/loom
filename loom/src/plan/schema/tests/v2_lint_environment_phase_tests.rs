//! The lints model which runner each phase's commands use, as
//! `verify::before_after` and `verify::completion` run them.

use super::*;

/// `run_before_stage_checks` always uses the confined runner with the network
/// denied: no session grant, stage confinement or disabled sandbox reaches it.
/// A plan that verified before this was modelled must still verify, so these
/// are warnings.
#[test]
fn before_stage_runs_in_the_confined_runner_whatever_the_plan_grants() {
    let mut grants_everything = plan_allowing(&["*"], &[]);
    grants_everything.loom.stages[0].sandbox.enabled = Some(false);
    let mut grants_nothing = plan_allowing(&[], &[]);
    grants_nothing.loom.sandbox.command_confinement = CommandConfinement::Confined;
    for mut metadata in [grants_everything, grants_nothing] {
        metadata.loom.stages[0].before_stage =
            crate::yaml::from_str("- command: bunx tsc\n- command: curl https://example.com")
                .expect("check");
        let (findings, _) = lint(&metadata, None);
        let registry: Vec<_> = findings
            .iter()
            .filter(|finding| finding.message.contains(NEEDS))
            .collect();
        assert_eq!(registry.len(), 1, "{findings:?}");
        assert!(registry[0].message.contains("before_stage check #1"));
        assert!(registry[0].message.contains("before any `provision` entry"));
        assert!(!registry[0].error_in_v2, "{findings:?}");
        let network: Vec<_> = findings
            .iter()
            .filter(|finding| finding.message.contains("before_stage check #2"))
            .collect();
        assert_eq!(network.len(), 1, "{findings:?}");
        assert!(network[0].message.contains("no network"), "{findings:?}");
        assert!(!network[0].error_in_v2, "{findings:?}");
    }
}

/// `run_after_checks` runs at the stage's resolved confinement. `inherit` keeps
/// the session rule, and `confined` denies the network like any completion
/// command. What the session rule refused stays an error; what it let through
/// becomes a warning.
#[test]
fn after_stage_follows_the_resolved_confinement() {
    let checks = "- command: bunx tsc\n- command: curl https://example.com";
    let findings_of = |domains: &[&str], enabled, confinement| {
        let mut metadata = plan_allowing(domains, &[]);
        metadata.loom.stages[0].after_stage = crate::yaml::from_str(checks).unwrap();
        metadata.loom.stages[0].sandbox.enabled = Some(enabled);
        metadata.loom.stages[0].sandbox.command_confinement = Some(confinement);
        lint(&metadata, None).0
    };
    let in_runner = |findings: &[LintFinding]| {
        let found: Vec<bool> = findings
            .iter()
            .filter(|finding| finding.message.contains("after_stage check #"))
            .inspect(|finding| {
                assert!(
                    finding.message.contains("confined"),
                    "names the runner: {}",
                    finding.message
                );
            })
            .map(|finding| finding.error_in_v2)
            .collect();
        found
    };

    let granted = findings_of(&["*"], true, CommandConfinement::Confined);
    assert_eq!(in_runner(&granted), [false, false], "{granted:?}");
    let unsandboxed = findings_of(&[], false, CommandConfinement::Confined);
    assert_eq!(in_runner(&unsandboxed), [false, false], "{unsandboxed:?}");
    let refused = findings_of(&[], true, CommandConfinement::Confined);
    assert_eq!(in_runner(&refused), [true, true], "{refused:?}");

    let inherited = findings_of(&["*"], true, CommandConfinement::Inherit);
    assert!(in_runner(&inherited).is_empty(), "{inherited:?}");
    let inherited_off = findings_of(&[], false, CommandConfinement::Inherit);
    assert!(in_runner(&inherited_off).is_empty(), "{inherited_off:?}");
}

/// Version probes: no network needed.
const CONFINED_PROBES: [&str; 7] = [
    "gh --version",
    "gh version",
    "curl --version",
    "wget -V",
    "/usr/bin/curl -V",
    "timeout 5 gh --version",
    "sh -c 'gh --version'",
];

/// Reachable only if the same command started the server.
const CONFINED_LOOPBACK: [&str; 6] = [
    "curl http://127.0.0.1:8080/health",
    "curl -fsS -o /dev/null -w '%{http_code}' http://localhost:$PORT/health",
    "curl -sSo/dev/null --max-time 5 http://[::1]:80/",
    "curl 127.0.0.1:8080",
    "wget -q -O - http://localhost/",
    "gh --version && curl http://127.0.0.1/",
];

/// A host the private network namespace cannot reach, or one it cannot prove local.
const CONFINED_REAL_HOSTS: [&str; 13] = [
    "curl https://example.com",
    "curl example.com",
    "curl -s example.com",
    "curl http://127.0.0.1@example.com/",
    "curl http://127.0.0.1.example.com/",
    "curl http://$HOST/",
    "curl http://127.0.0.1$(host)/",
    "curl http://127.0.0.1/ https://example.com/",
    "curl --proxy http://example.com http://127.0.0.1/",
    "curl http://127.0.0.1/ && sh -c 'curl https://example.com'",
    "$(curl https://example.com)",
    "curl -V https://example.com",
    "gh pr view",
];

/// `command`'s acceptance-criterion findings under a plan granting a domain and
/// under one granting none.
fn confined_network_findings(command: &str) -> [Vec<LintFinding>; 2] {
    let granted = confined_plan_allowing(&["example.com"], &[command]);
    let mut no_domains = confined_plan_allowing(&[], &[command]);
    no_domains.loom.stages[0].sandbox.enabled = Some(true);
    [granted, no_domains].map(|metadata| {
        let (findings, _) = lint(&metadata, None);
        findings
            .into_iter()
            .filter(|finding| finding.message.contains("Acceptance criterion #1"))
            .collect()
    })
}

/// The confined runner has a private network namespace with only its own
/// loopback up: a version probe needs no network and a loopback URL reaches a
/// server the same command started, so neither is a hard error there. Any other
/// host cannot be reached, and stays one.
#[test]
fn a_confined_probe_that_can_run_is_no_error_and_a_real_host_still_is() {
    for command in CONFINED_PROBES {
        for findings in confined_network_findings(command) {
            assert!(findings.is_empty(), "{command}: {findings:?}");
        }
    }
    for command in CONFINED_LOOPBACK {
        for findings in confined_network_findings(command) {
            assert_eq!(findings.len(), 1, "{command}: {findings:?}");
            assert!(!findings[0].error_in_v2, "{command}: {findings:?}");
            let message = &findings[0].message;
            assert!(
                message.contains("server the same command starts"),
                "{message}"
            );
        }
    }
    for command in CONFINED_REAL_HOSTS {
        for findings in confined_network_findings(command) {
            assert_eq!(findings.len(), 1, "{command}: {findings:?}");
            assert!(findings[0].error_in_v2, "{command}: {findings:?}");
            assert!(
                findings[0].message.contains("confined completion runner"),
                "{command}: {findings:?}"
            );
        }
    }
}

/// `allowed_domains` cannot reach a confined command, so the finding must not
/// send the author there, with or without domains.
#[test]
fn a_confined_network_finding_names_a_remedy_that_works() {
    let mut no_domains = confined_plan_allowing(&[], &["curl https://example.com"]);
    no_domains.loom.stages[0].sandbox.enabled = Some(true);
    let granted = confined_plan_allowing(&["example.com"], &["curl https://example.com"]);
    for metadata in [no_domains, granted] {
        let found = messages(&metadata, None, "`curl`");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("confined completion runner"), "{found:?}");
        assert!(
            found[0].contains("whatever `allowed_domains` says"),
            "{found:?}"
        );
        assert!(
            found[0].contains("`command_confinement: inherit`"),
            "{found:?}"
        );
        assert!(found[0].contains("`provision`"), "{found:?}");
        assert!(!found[0].contains("add the host"), "{found:?}");
    }
}

/// The session sandbox is a different runner: a probe there keeps its finding.
#[test]
fn a_session_command_keeps_its_network_finding() {
    for command in ["curl http://127.0.0.1/", "gh --version"] {
        let found = messages(
            &plan_allowing(&[], &[command]),
            None,
            "allows no network domain",
        );
        assert_eq!(found.len(), 1, "{command}: {found:?}");
    }
}

#[test]
fn a_confined_setup_command_is_an_error_and_a_confined_after_stage_check_a_warning() {
    let mut metadata = confined_plan_allowing(&["registry.npmjs.org"], &[]);
    metadata.loom.stages[0].setup = vec!["npm ci".to_string()];
    metadata.loom.stages[0].after_stage =
        crate::yaml::from_str("- command: bunx tsc").expect("check");
    let (findings, _) = lint(&metadata, None);
    let found: Vec<_> = findings
        .iter()
        .filter(|finding| finding.message.contains(NEEDS))
        .collect();
    assert_eq!(found.len(), 2, "{findings:?}");
    assert!(found[0].message.contains("Setup command #1"), "{found:?}");
    assert!(found[0].error_in_v2, "{found:?}");
    assert!(
        found[1].message.contains("after_stage check #1"),
        "{found:?}"
    );
    assert!(
        found[1].message.contains("no network whatever"),
        "{found:?}"
    );
    assert!(!found[1].error_in_v2, "{found:?}");
}
