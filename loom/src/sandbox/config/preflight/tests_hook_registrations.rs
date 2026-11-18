//! Hard-linked hook targets, and loom hook registrations read from the
//! project's own settings documents.

use super::*;

#[test]
fn check_5_warns_about_a_hard_linked_hook_without_refusing_it() {
    let scene = scene();
    let hook = scene.outside.join("notify.sh");
    executable(&hook);
    std::fs::hard_link(&hook, scene.outside.join("second-name")).unwrap();

    let findings = scan(&scene, &[document("Stop", &hook.display().to_string())]);

    assert!(findings.refusals.is_empty(), "{findings:?}");
    assert_eq!(findings.warnings.len(), 1, "{findings:?}");
    assert!(
        findings.warnings[0].contains("2 hard links"),
        "{findings:?}"
    );
}

#[test]
fn a_loom_hook_registration_resolving_inside_the_repository_is_refused() {
    let scene = scene();
    let in_repo = scene.repo.join("loom-hooks").join("commit-guard.sh");
    let installed = scene.outside.join("hooks").join("commit-guard.sh");
    executable(&in_repo);
    executable(&installed);
    let settings_dir = scene.repo.join(".claude");
    std::fs::create_dir_all(&settings_dir).unwrap();
    for (name, script) in [
        ("settings.json", &installed),
        ("settings.local.json", &in_repo),
    ] {
        let command = format!("/bin/bash {}", script.display());
        let settings = document("PreToolUse", &command).settings.to_string();
        std::fs::write(settings_dir.join(name), settings).unwrap();
    }

    let documents = hook_documents(&scene.repo, Some(scene.temp.path()));
    assert_eq!(
        documents.len(),
        2,
        "both project documents are read, no home one exists"
    );
    let findings = scan(&scene, &documents);

    assert_eq!(findings.refusals.len(), 1, "{findings:?}");
    let refusal = &findings.refusals[0];
    assert!(refusal.starts_with("loom hook `PreToolUse`"), "{refusal}");
    assert!(refusal.contains("settings.local.json"), "{refusal}");
    assert!(refusal.contains("inside the repository"), "{refusal}");
}
