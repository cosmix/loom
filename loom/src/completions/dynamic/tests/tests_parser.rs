use super::super::{extract_command_path, parse_cmdline_words};

fn path(command_line: &str, current_word: &str) -> Vec<String> {
    extract_command_path(&parse_cmdline_words(command_line), current_word)
}

#[test]
fn quoted_option_values_are_one_word_and_not_subcommands() {
    assert_eq!(
        parse_cmdline_words("loom stage retry core --context 'two words'"),
        ["stage", "retry", "core", "--context", "two words"]
    );
    assert_eq!(
        path("loom stage retry core --context 'two words'", ""),
        ["stage", "retry"]
    );
}

#[test]
fn valued_options_do_not_become_command_path_segments() {
    assert_eq!(
        path("loom knowledge context --query merge", ""),
        ["knowledge", "context"]
    );
    assert_eq!(
        path("loom run --max-parallel 4 --backend tmux", ""),
        ["run"]
    );
}

#[test]
fn positional_values_do_not_become_subcommands() {
    assert_eq!(path("loom stage complete core", ""), ["stage", "complete"]);
    assert_eq!(path("loom init doc/plans/PLAN-x.md", ""), ["init"]);
}

#[test]
fn current_word_is_removed_only_from_the_end() {
    assert_eq!(
        path("loom stage output output", "output"),
        ["stage", "output"]
    );
}
