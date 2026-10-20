//! The forward wrapper's spelling in the signal, the capsule's allow rule and
//! the receipt parser agree when `~/.claude` is a symlink: the allow rule and
//! the parser name the canonical path, as the shell guard's `pwd -P`
//! comparison does, and the signal's `~` spelling is accepted too.

use std::fs;

use serial_test::serial;

use crate::assets::install::test_env::InstallRootsEnv;
use crate::fs::permissions::settings::codex_forward_home_allow_entry;
use crate::models::forward_receipt::transcript::exact_forward_argv;

const ARGS: &str = "task 'work' --model gpt-5.6-terra --effort xhigh --write";
const TILDE_WRAPPER: &str = "~/.claude/hooks/loom/codex-forward.sh";

#[test]
#[serial]
fn a_symlinked_claude_root_is_spelled_canonical_everywhere() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let home = base.join("home");
    let real = base.join("dotfiles-claude");
    fs::create_dir_all(real.join("hooks/loom")).unwrap();
    fs::create_dir_all(&home).unwrap();
    std::os::unix::fs::symlink(&real, home.join(".claude")).unwrap();
    let _env = InstallRootsEnv::isolate(&home);

    let wrapper = real
        .join("hooks/loom/codex-forward.sh")
        .display()
        .to_string();
    assert_eq!(
        codex_forward_home_allow_entry(),
        Some(format!("Bash({wrapper}:*)"))
    );
    assert!(exact_forward_argv(&format!("{wrapper} {ARGS}")).is_some());
    assert!(exact_forward_argv(&format!("{TILDE_WRAPPER} {ARGS}")).is_some());
}

#[test]
fn the_signal_names_the_tilde_spelling() {
    let mut content = String::new();
    super::push_codex_spawn_rules(&mut content);
    assert!(content.contains(&format!("`{TILDE_WRAPPER} task '<task>'")));
}
