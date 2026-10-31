//! The `.npmrc` refusal of a provision install only counts when nothing moves the
//! install out of the directory the refusal checked, and nothing lets the install
//! run whether or not the refusal passed.

use super::v2_tests::provision_messages;

const REFUSAL: &str = "test ! -e .npmrc && test ! -L .npmrc";
const BUN_INSTALL: &str =
    "bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null";

/// `command` raises exactly one error: its bun install, with every flag but the
/// `.npmrc` refusal, which the message words as staying in the install's directory.
fn assert_names_the_npmrc_refusal(command: &str) {
    let messages = provision_messages(command);
    assert_eq!(messages.len(), 1, "{command:?}: {messages:?}");
    let expected =
        format!("provision entry #1 runs `bun install` without a leading `{REFUSAL} &&`");
    assert!(
        messages[0].starts_with(&expected),
        "{command:?}: {messages:?}"
    );
    assert!(messages[0].contains("with no `cd`"), "{messages:?}");
}

#[test]
fn a_directory_change_voids_the_npmrc_refusal() {
    for command in [
        format!("{REFUSAL} && cd web && {BUN_INSTALL}"),
        format!("{REFUSAL} && pushd web && {BUN_INSTALL}"),
        format!("{REFUSAL} && popd && {BUN_INSTALL}"),
        format!("{REFUSAL} && builtin cd web && {BUN_INSTALL}"),
        format!("{REFUSAL} && sh -c 'cd web && {BUN_INSTALL}'"),
        format!("pushd web && {REFUSAL} && {BUN_INSTALL}"),
    ] {
        assert_names_the_npmrc_refusal(&command);
    }
}

#[test]
fn a_refusal_the_install_can_outrun_is_an_error() {
    for command in [
        format!("{REFUSAL} | {BUN_INSTALL}"),
        format!("{REFUSAL} & {BUN_INSTALL}"),
        format!("({REFUSAL}); {BUN_INSTALL}"),
    ] {
        assert_names_the_npmrc_refusal(&command);
    }
}

#[test]
fn a_grouped_install_and_a_cd_argument_keep_the_refusal() {
    for command in [
        format!("({REFUSAL} && {BUN_INSTALL})"),
        format!("{REFUSAL} && {BUN_INSTALL} && echo cd web"),
    ] {
        let messages = provision_messages(&command);
        assert!(messages.is_empty(), "{command:?}: {messages:?}");
    }
}

#[test]
fn install_flags_cannot_be_overridden_or_hidden_after_terminator() {
    for command in [
        format!("{REFUSAL} && {BUN_INSTALL} --backend=hardlink"),
        format!("{REFUSAL} && {BUN_INSTALL} --config /tmp/config"),
        format!("{REFUSAL} && {BUN_INSTALL} --no-ignore-scripts"),
        format!("{REFUSAL} && {BUN_INSTALL} -- --config=/tmp/config"),
        format!("{REFUSAL} && npm ci --ignore-scripts --ignore-scripts=false"),
        format!(
            "{REFUSAL} && pnpm install --ignore-scripts --ignore-pnpmfile --no-ignore-pnpmfile"
        ),
        format!("{REFUSAL} && yarn install -- --ignore-scripts"),
        "uv sync --no-install-project --no-install-project=false".to_string(),
        format!("{REFUSAL} && bun --cwd web install --backend=hardlink"),
    ] {
        assert!(!provision_messages(&command).is_empty(), "{command}");
    }
    for command in [
        format!("{REFUSAL} && bun install --ignore-scripts --backend copyfile --config /dev/null"),
        format!("{REFUSAL} && npm ci --ignore-scripts -- package"),
        format!("{REFUSAL} && pnpm install --ignore-scripts --ignore-pnpmfile"),
        format!("{REFUSAL} && yarn install --ignore-scripts"),
        "uv sync --no-install-project".to_string(),
    ] {
        assert!(provision_messages(&command).is_empty(), "{command}");
    }
}
