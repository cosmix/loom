//! The forward wrapper's spelling in the signal, the capsule's allow rule and
//! the receipt parser agree on one canonical path, as the shell guard's
//! `pwd -P` comparison does.

use std::fs;

use serial_test::serial;

use super::forward_wrapper_spelling;
use crate::assets::install::test_env::{InstallRootsEnv, CLAUDE_OVERRIDE};
use crate::fs::permissions::settings::codex_forward_home_allow_entry;
use crate::models::forward_receipt::transcript::exact_forward_argv;

const ARGS: &str = "task 'work' --model gpt-5.6-terra --effort xhigh --write";

#[test]
#[serial]
fn a_symlinked_relocated_root_is_spelled_canonical_everywhere() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let home = base.join("home");
    let real = base.join("real-claude");
    fs::create_dir_all(real.join("hooks/loom")).unwrap();
    fs::create_dir_all(&home).unwrap();
    let link = base.join("claude-link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let mut env = InstallRootsEnv::isolate(&home);
    env.set(CLAUDE_OVERRIDE, &link);

    let wrapper = real
        .join("hooks/loom/codex-forward.sh")
        .display()
        .to_string();
    assert_eq!(forward_wrapper_spelling(), wrapper);
    assert_eq!(
        codex_forward_home_allow_entry(),
        Some(format!("Bash({wrapper}:*)"))
    );
    assert!(exact_forward_argv(&format!("{wrapper} {ARGS}")).is_some());
    let lexical = link.join("hooks/loom/codex-forward.sh");
    assert_eq!(
        exact_forward_argv(&format!("{} {ARGS}", lexical.display())),
        None
    );
    assert_eq!(
        exact_forward_argv(&format!("~/.claude/hooks/loom/codex-forward.sh {ARGS}")),
        None
    );
}

#[test]
#[serial]
fn the_default_root_keeps_the_tilde_spelling() {
    let temp = tempfile::tempdir().unwrap();
    let _env = InstallRootsEnv::isolate(temp.path());

    let spelling = forward_wrapper_spelling();
    assert_eq!(spelling, "~/.claude/hooks/loom/codex-forward.sh");
    assert!(exact_forward_argv(&format!("{spelling} {ARGS}")).is_some());
}
