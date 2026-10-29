use super::*;
use crate::fs::work_dir::tests::{init_work, write_user_config};
use std::fs;
use tempfile::TempDir;

#[test]
fn global_config_tier_terminal_backend_falls_through_to_user_config_native() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[terminal]\nbackend = \"native\"\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);

    // Workspace [terminal] absent: falls through to the user config. Tmux is
    // the built-in default, so a reader that ignored the user config fails here.
    let config = read_terminal_config(&work).unwrap();
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Native
    );

    // A workspace [terminal] that sets backend wins over the user config.
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: crate::models::session::SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    let config = read_terminal_config(&work).unwrap();
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Tmux
    );
}

#[test]
fn global_config_tier_an_empty_workspace_terminal_section_inherits_native() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[terminal]\nbackend = \"native\"\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);
    fs::write(work.join("config.toml"), "[terminal]\n").unwrap();

    let config = read_terminal_config(&work).unwrap();
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Native
    );
}
